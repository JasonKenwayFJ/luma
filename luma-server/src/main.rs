use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, Mutex};
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;

#[tokio::main]
async fn main() {
    let listener = TcpListener::bind("127.0.0.1:8080").await.unwrap();
    println!("Luma WS relay running on ws://127.0.0.1:8080");

    let (tx, _rx) = broadcast::channel::<(usize, String)>(100);
    let next_id = Arc::new(Mutex::new(0usize));

    loop {
        let (stream, _) = listener.accept().await.unwrap();
        let tx = tx.clone();
        let rx = tx.subscribe();
        let next_id = next_id.clone();

        tokio::spawn(async move {
            let mut id_guard = next_id.lock().await;
            let my_id = *id_guard;
            *id_guard += 1;
            drop(id_guard);

            handle_connection(stream, my_id, tx, rx).await;
        });
    }
}

async fn handle_connection(
    stream: TcpStream,
    my_id: usize,
    tx: broadcast::Sender<(usize, String)>,
    mut rx: broadcast::Receiver<(usize, String)>,
) {
    let ws_stream = match accept_async(stream).await {
        Ok(ws) => ws,
        Err(_) => return,
    };
    let (mut write, mut read) = ws_stream.split();

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
                let _ = tx.send((my_id, text));
            }
            Ok(Message::Close(_)) | Err(_) => break,
            _ => {}
        }
    }

    write_task.abort();
}