use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, Mutex};
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;

const LEGACY_HISTORY_FILE: &str = "history.jsonl";
const HISTORY_LIMIT: i64 = 200;
const MAX_TEXT_LEN: usize = 4000;

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct WireMessage {
    id: String,
    user_id: String,
    room_id: String,
    author_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    avatar_url: Option<String>,
    #[serde(default)]
    text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    attachments: Option<serde_json::Value>,
    sent_at: String,
}

fn is_valid(m: &WireMessage) -> bool {
    !m.id.is_empty()
        && !m.user_id.is_empty()
        && !m.room_id.is_empty()
        && m.author_name.chars().count() <= 50
        && m.text.chars().count() <= MAX_TEXT_LEN
        && (!m.text.is_empty() || m.attachments.is_some())
}

// ---------- База данных (Postgres / Neon) ----------

async fn init_db() -> PgPool {
    let url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL is not set (create luma-server/.env)");

    // acquire_timeout с запасом: бесплатный compute в Neon засыпает
    // при простое, и первое подключение после паузы занимает пару секунд.
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(15))
        .connect(&url)
        .await
        .expect("cannot connect to database");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS messages (
            id          TEXT PRIMARY KEY,
            room_id     TEXT NOT NULL,
            user_id     TEXT NOT NULL,
            author_name TEXT NOT NULL,
            avatar_url  TEXT,
            text        TEXT NOT NULL,
            attachments JSONB,
            sent_at     TIMESTAMPTZ NOT NULL,
            created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
        )",
    )
        .execute(&pool)
        .await
        .expect("cannot create table");

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_messages_room_sent
         ON messages (room_id, sent_at)",
    )
        .execute(&pool)
        .await
        .expect("cannot create index");

    pool
}

// true = сообщение новое, false = такой id уже был.
async fn save_message(pool: &PgPool, m: &WireMessage) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "INSERT INTO messages
            (id, room_id, user_id, author_name, avatar_url, text, attachments, sent_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8::timestamptz)
         ON CONFLICT (id) DO NOTHING",
    )
        .bind(&m.id)
        .bind(&m.room_id)
        .bind(&m.user_id)
        .bind(&m.author_name)
        .bind(m.avatar_url.as_deref())
        .bind(&m.text)
        .bind(m.attachments.clone())
        .bind(&m.sent_at)
        .execute(pool)
        .await?;

    Ok(result.rows_affected() > 0)
}

// Последние N сообщений по всем комнатам, от старых к новым.
async fn load_recent(pool: &PgPool, limit: i64) -> Vec<WireMessage> {
    // to_char возвращает время в том же виде, что JS toISOString():
    // 2026-09-28T18:16:00.123Z. Клиент сортирует эти строки как текст,
    // поэтому формат должен быть строго фиксированной длины.
    let rows = sqlx::query(
        "SELECT recent.id, recent.room_id, recent.user_id, recent.author_name,
                recent.avatar_url, recent.text, recent.attachments,
                to_char(recent.sent_at AT TIME ZONE 'UTC',
                        'YYYY-MM-DD\"T\"HH24:MI:SS.MS\"Z\"') AS sent_at
         FROM (
             SELECT * FROM messages
             ORDER BY sent_at DESC, created_at DESC
             LIMIT $1
         ) recent
         ORDER BY recent.sent_at ASC, recent.created_at ASC",
    )
        .bind(limit)
        .fetch_all(pool)
        .await;

    match rows {
        Ok(rows) => rows
            .into_iter()
            .map(|r| WireMessage {
                id: r.get("id"),
                user_id: r.get("user_id"),
                room_id: r.get("room_id"),
                author_name: r.get("author_name"),
                avatar_url: r.get("avatar_url"),
                text: r.get("text"),
                attachments: r.get("attachments"),
                sent_at: r.get("sent_at"),
            })
            .collect(),
        Err(e) => {
            eprintln!("db read error: {e}");
            Vec::new()
        }
    }
}

// Разовый перенос старого history.jsonl, если он лежит рядом.
async fn import_legacy_history(pool: &PgPool) {
    let Ok(content) = std::fs::read_to_string(LEGACY_HISTORY_FILE) else {
        return;
    };

    let mut imported = 0;
    for line in content.lines() {
        if let Ok(m) = serde_json::from_str::<WireMessage>(line) {
            if is_valid(&m) && matches!(save_message(pool, &m).await, Ok(true)) {
                imported += 1;
            }
        }
    }

    std::fs::rename(
        LEGACY_HISTORY_FILE,
        format!("{LEGACY_HISTORY_FILE}.imported"),
    )
        .ok();
    println!("Imported {imported} messages from {LEGACY_HISTORY_FILE}");
}

// ---------- Сервер ----------

#[tokio::main]
async fn main() {
    // Читает luma-server/.env в переменные окружения процесса.
    // Если файла нет, не страшно: на хостинге переменная задаётся напрямую.
    dotenvy::dotenv().ok();

    let pool = init_db().await;
    import_legacy_history(&pool).await;
    println!("DB ready (Postgres)");

    let listener = TcpListener::bind("127.0.0.1:8080").await.unwrap();
    println!("Luma WS relay running on ws://127.0.0.1:8080");

    let (tx, _rx) = broadcast::channel::<(usize, String)>(100);
    let next_id = Arc::new(Mutex::new(0usize));

    loop {
        let (stream, _) = listener.accept().await.unwrap();
        let tx = tx.clone();
        let rx = tx.subscribe();
        let next_id = next_id.clone();
        let pool = pool.clone();

        tokio::spawn(async move {
            let mut id_guard = next_id.lock().await;
            let my_id = *id_guard;
            *id_guard += 1;
            drop(id_guard);

            handle_connection(stream, my_id, tx, rx, pool).await;
        });
    }
}

async fn handle_connection(
    stream: TcpStream,
    my_id: usize,
    tx: broadcast::Sender<(usize, String)>,
    mut rx: broadcast::Receiver<(usize, String)>,
    pool: PgPool,
) {
    let ws_stream = match accept_async(stream).await {
        Ok(ws) => ws,
        Err(_) => return,
    };
    let (mut write, mut read) = ws_stream.split();

    let recent = load_recent(&pool, HISTORY_LIMIT).await;
    if !recent.is_empty() {
        let json = serde_json::to_string(&recent).unwrap();
        if write.send(Message::Text(json)).await.is_err() {
            return;
        }
    }

    let write_task = tokio::spawn(async move {
        while let Ok((sender_id, text)) = rx.recv().await {
            if sender_id != my_id {
                if write.send(Message::Text(text)).await.is_err() {
                    break;
                }
            }
        }
    });

    while let Some(msg) = read.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                let Ok(m) = serde_json::from_str::<WireMessage>(&text) else {
                    continue;
                };
                if !is_valid(&m) {
                    continue;
                }

                match save_message(&pool, &m).await {
                    Ok(true) => {
                        let json = serde_json::to_string(&m).unwrap();
                        let _ = tx.send((my_id, json));
                    }
                    Ok(false) => {}
                    Err(e) => eprintln!("db write error: {e}"),
                }
            }
            Ok(Message::Close(_)) | Err(_) => break,
            _ => {}
        }
    }

    write_task.abort();
}