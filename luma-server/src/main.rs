use futures_util::{SinkExt, StreamExt};
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, Mutex};
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;

const HISTORY_LIMIT: usize = 100;
const HISTORY_FILE: &str = "history.jsonl";
type History = Arc<Mutex<VecDeque<String>>>;

fn is_json_object(s: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(s)
        .map(|v| v.is_object())
        .unwrap_or(false)
}

// Читаем файл при старте: оставляем валидные строки, обрезаем до лимита
// и сразу перезаписываем файл урезанной версией.
fn load_history() -> VecDeque<String> {
    let content = match std::fs::read_to_string(HISTORY_FILE) {
        Ok(c) => c,
        Err(_) => return VecDeque::new(),
    };

    let mut history: VecDeque<String> = content
        .lines()
        .filter(|l| is_json_object(l))
        .map(|l| l.to_string())
        .collect();

    while history.len() > HISTORY_LIMIT {
        history.pop_front();
    }

    let compacted: String = history.iter().map(|l| format!("{l}\n")).collect();
    std::fs::write(HISTORY_FILE, compacted).ok();

    println!("Loaded {} messages from {}", history.len(), HISTORY_FILE);
    history
}

async fn append_to_file(line: &str) {
    match OpenOptions::new()
        .create(true)
        .append(true)
        .open(HISTORY_FILE)
        .await
    {
        Ok(mut f) => {
            if let Err(e) = f.write_all(format!("{line}\n").as_bytes()).await {
                eprintln!("history write error: {e}");
            }
        }
        Err(e) => eprintln!("history open error: {e}"),
    }
}

#[tokio::main]
async fn main() {
    let listener = TcpListener::bind("127.0.0.1:8080").await.unwrap();
    println!("Luma WS relay running on ws://127.0.0.1:8080");

    let (tx, _rx) = broadcast::channel::<(usize, String)>(100);
    let next_id = Arc::new(Mutex::new(0usize));
    let history: History = Arc::new(Mutex::new(load_history()));

    loop {
        let (stream, _) = listener.accept().await.unwrap();
        let tx = tx.clone();
        let rx = tx.subscribe();
        let next_id = next_id.clone();
        let history = history.clone();

        tokio::spawn(async move {
            let mut id_guard = next_id.lock().await;
            let my_id = *id_guard;
            *id_guard += 1;
            drop(id_guard);

            handle_connection(stream, my_id, tx, rx, history).await;
        });
    }
}

async fn handle_connection(
    stream: TcpStream,
    my_id: usize,
    tx: broadcast::Sender<(usize, String)>,
    mut rx: broadcast::Receiver<(usize, String)>,
    history: History,
) {
    let ws_stream = match accept_async(stream).await {
        Ok(ws) => ws,
        Err(_) => return,
    };
    let (mut write, mut read) = ws_stream.split();

    let snapshot = {
        let h = history.lock().await;
        if h.is_empty() {
            None
        } else {
            let items: Vec<&str> = h.iter().map(|s| s.as_str()).collect();
            Some(format!("[{}]", items.join(",")))
        }
    };
    if let Some(json) = snapshot {
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
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
                    if value.is_object() {
                        // Компактная однострочная форма: гарантия, что
                        // одно сообщение = одна строка файла.
                        let line = value.to_string();

                        let mut h = history.lock().await;
                        h.push_back(line.clone());
                        if h.len() > HISTORY_LIMIT {
                            h.pop_front();
                        }
                        // Пишем в файл, пока держим замок: порядок строк
                        // в файле совпадает с порядком в памяти.
                        append_to_file(&line).await;
                    }
                }
                let _ = tx.send((my_id, text));
            }
            Ok(Message::Close(_)) | Err(_) => break,
            _ => {}
        }
    }

    write_task.abort();
}