use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use sqlx::postgres::{PgPoolOptions, PgRow};
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

// Кадры от клиента. Внутренне тегированный enum: serde читает поле "type",
// а остальные поля объекта разбирает как содержимое нужного варианта.
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum ClientFrame {
    Join {
        #[serde(rename = "roomId")]
        room_id: String,
    },
    Leave,
    Send(WireMessage),
}

// Кадры к клиенту, тем же способом, только на сериализацию.
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum ServerFrame {
    Summary { messages: Vec<WireMessage> },
    History {
        #[serde(rename = "roomId")]
        room_id: String,
        messages: Vec<WireMessage>,
    },
    Message(WireMessage),
}

fn is_valid(m: &WireMessage) -> bool {
    !m.id.is_empty()
        && !m.user_id.is_empty()
        && !m.room_id.is_empty()
        && m.author_name.chars().count() <= 50
        && m.text.chars().count() <= MAX_TEXT_LEN
        && (!m.text.is_empty() || m.attachments.is_some())
}

// ---------- База данных ----------

async fn init_db() -> PgPool {
    let url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL is not set (create luma-server/.env)");

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

fn row_to_message(r: &PgRow) -> WireMessage {
    WireMessage {
        id: r.get("id"),
        user_id: r.get("user_id"),
        room_id: r.get("room_id"),
        author_name: r.get("author_name"),
        avatar_url: r.get("avatar_url"),
        text: r.get("text"),
        attachments: r.get("attachments"),
        sent_at: r.get("sent_at"),
    }
}

const SENT_AT_EXPR: &str =
    "to_char(sent_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.MS\"Z\"') AS sent_at";

// Последние N сообщений ОДНОЙ комнаты. Берём свежие первыми (удобно для
// LIMIT), а затем переворачиваем в Rust, чтобы клиенту прийти по
// возрастанию времени — без вложенных подзапросов ради этого в SQL.
async fn load_room_history(pool: &PgPool, room_id: &str, limit: i64) -> Vec<WireMessage> {
    let sql = format!(
        "SELECT id, room_id, user_id, author_name, avatar_url, text, attachments, {SENT_AT_EXPR}
         FROM messages
         WHERE room_id = $1
         ORDER BY sent_at DESC, created_at DESC
         LIMIT $2"
    );
    let rows = sqlx::query(&sql).bind(room_id).bind(limit).fetch_all(pool).await;

    match rows {
        Ok(rows) => {
            let mut messages: Vec<WireMessage> = rows.iter().map(row_to_message).collect();
            messages.reverse();
            messages
        }
        Err(e) => {
            eprintln!("db read error (history): {e}");
            Vec::new()
        }
    }
}

// По одному последнему сообщению на каждую комнату — для превью в списке
// чатов, ещё до того как пользователь куда-то зашёл.
async fn load_summary(pool: &PgPool) -> Vec<WireMessage> {
    let sql = format!(
        "SELECT DISTINCT ON (room_id)
                id, room_id, user_id, author_name, avatar_url, text, attachments, {SENT_AT_EXPR}
         FROM messages
         ORDER BY room_id, sent_at DESC, created_at DESC"
    );
    let rows = sqlx::query(&sql).fetch_all(pool).await;

    match rows {
        Ok(rows) => rows.iter().map(row_to_message).collect(),
        Err(e) => {
            eprintln!("db read error (summary): {e}");
            Vec::new()
        }
    }
}

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

    std::fs::rename(LEGACY_HISTORY_FILE, format!("{LEGACY_HISTORY_FILE}.imported")).ok();
    println!("Imported {imported} messages from {LEGACY_HISTORY_FILE}");
}

// ---------- Сервер ----------

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let pool = init_db().await;
    import_legacy_history(&pool).await;
    println!("DB ready (Postgres)");

    let listener = TcpListener::bind("127.0.0.1:8080").await.unwrap();
    println!("Luma WS relay running on ws://127.0.0.1:8080");

    // (sender_id, room_id, JSON-кадр): каждое соединение само решает,
    // интересна ли ему эта комната, при получении из broadcast.
    let (tx, _rx) = broadcast::channel::<(usize, String, String)>(200);
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
    tx: broadcast::Sender<(usize, String, String)>,
    mut rx: broadcast::Receiver<(usize, String, String)>,
    pool: PgPool,
) {
    let ws_stream = match accept_async(stream).await {
        Ok(ws) => ws,
        Err(_) => return,
    };
    let (mut write, mut read) = ws_stream.split();

    // Сводка сразу при подключении, ещё до всякого join.
    let summary = load_summary(&pool).await;
    if !summary.is_empty() {
        let json = serde_json::to_string(&ServerFrame::Summary { messages: summary }).unwrap();
        if write.send(Message::Text(json)).await.is_err() {
            return;
        }
    }

    // Комната, которую сейчас смотрит этот клиент. None — он в списке
    // чатов, живые сообщения ему пока не нужны.
    let mut current_room: Option<String> = None;

    loop {
        tokio::select! {
            incoming = read.next() => {
                match incoming {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<ClientFrame>(&text) {
                            Ok(ClientFrame::Join { room_id }) => {
                                let messages = load_room_history(&pool, &room_id, HISTORY_LIMIT).await;
                                let frame = ServerFrame::History { room_id: room_id.clone(), messages };
                                current_room = Some(room_id);
                                let json = serde_json::to_string(&frame).unwrap();
                                if write.send(Message::Text(json)).await.is_err() {
                                    break;
                                }
                            }
                            Ok(ClientFrame::Leave) => {
                                current_room = None;
                            }
                            Ok(ClientFrame::Send(m)) => {
                                if is_valid(&m) {
                                    match save_message(&pool, &m).await {
                                        Ok(true) => {
                                            let room_id = m.room_id.clone();
                                            let json =
                                                serde_json::to_string(&ServerFrame::Message(m)).unwrap();
                                            let _ = tx.send((my_id, room_id, json));
                                        }
                                        Ok(false) => {}
                                        Err(e) => eprintln!("db write error: {e}"),
                                    }
                                }
                            }
                            Err(e) => eprintln!("bad client frame: {e}"),
                        }
                    }
                    Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                    _ => {}
                }
            }

            outgoing = rx.recv() => {
                match outgoing {
                    Ok((sender_id, room_id, json)) => {
                        // Своё эхо не шлём, чужое — только если сейчас в этой же комнате.
                        if sender_id != my_id && current_room.as_deref() == Some(room_id.as_str()) {
                            if write.send(Message::Text(json)).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(_) => {} // это соединение отстало от канала — пропускаем, не критично для MVP
                }
            }
        }
    }
}