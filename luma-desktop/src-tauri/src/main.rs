#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use futures_util::{SinkExt, StreamExt};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Emitter, Manager};
use tauri_plugin_notification::NotificationExt;
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};

struct WsState {
    tx: mpsc::UnboundedSender<String>,
    connected: Arc<AtomicBool>,
    // Последняя история, полученная от сервера. Нужна, чтобы React мог
    // забрать её, даже если она пришла раньше, чем он подписался на события.
    history: Arc<Mutex<Option<String>>>,
}

#[tauri::command]
fn send_message(text: String, state: tauri::State<WsState>) -> Result<(), String> {
    state.tx.send(text).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_connection_status(state: tauri::State<WsState>) -> bool {
    state.connected.load(Ordering::SeqCst)
}

#[tauri::command]
fn get_history(state: tauri::State<WsState>) -> Option<String> {
    state.history.lock().unwrap().clone()
}
fn notification_body(raw: &str) -> String {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) else {
        return raw.to_string();
    };
    let author = v["authorName"].as_str().unwrap_or("Luma");
    let text = v["text"].as_str().unwrap_or("");
    if text.is_empty() {
        format!("{author}: 📎 Вложение")
    } else {
        format!("{author}: {text}")
    }
}
fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let app_handle = app.handle().clone();

            let (tx, mut rx) = mpsc::unbounded_channel::<String>();
            let connected = Arc::new(AtomicBool::new(false));
            let history: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
            app.manage(WsState {
                tx,
                connected: connected.clone(),
                history: history.clone(),
            });

            tauri::async_runtime::spawn(async move {
                let url = "ws://127.0.0.1:8080";

                loop {
                    match connect_async(url).await {
                        Ok((ws_stream, _)) => {
                            println!("WS connected");
                            connected.store(true, Ordering::SeqCst);
                            app_handle.emit("ws-status", true).ok();

                            let (mut write, mut read) = ws_stream.split();

                            loop {
                                tokio::select! {
                                    outgoing = rx.recv() => {
                                        match outgoing {
                                            Some(msg) => {
                                                if write.send(Message::Text(msg)).await.is_err() {
                                                    break;
                                                }
                                            }
                                            None => return,
                                        }
                                    }

                                    incoming = read.next() => {
                                        match incoming {
                                            Some(Ok(Message::Text(text))) => {
                                                if text.starts_with('[') {
                                                    // Сначала запоминаем, потом сообщаем.
                                                    *history.lock().unwrap() = Some(text.clone());
                                                    app_handle.emit("ws-history", text).ok();
                                                } else {
                                                    let is_visible = app_handle
                                                        .get_webview_window("main")
                                                        .map(|w| w.is_visible().unwrap_or(false))
                                                        .unwrap_or(false);

                                                    app_handle.emit("ws-message", text.clone()).ok();

                                                    if !is_visible {
                                                        app_handle
                                                            .notification()
                                                            .builder()
                                                            .title("Luma")
                                                            .body(notification_body(&text))
                                                            .show()
                                                            .ok();
                                                    }
                                                }
                                            }
                                            Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                                            _ => {}
                                        }
                                    }
                                }
                            }

                            println!("WS disconnected");
                            connected.store(false, Ordering::SeqCst);
                            app_handle.emit("ws-status", false).ok();
                        }
                        Err(e) => eprintln!("WS connect error: {e}"),
                    }

                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                }
            });

            // --- Tray ---
            let open_item = MenuItem::with_id(app, "open", "Открыть", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Выйти", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open_item, &quit_item])?;

            TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => app.exit(0),
                    "open" => {
                        if let Some(w) = app.get_webview_window("main") {
                            w.show().ok();
                            w.set_focus().ok();
                        }
                    }
                    _ => {}
                })
                .build(app)?;

            // --- Скрытие в трей вместо закрытия ---
            let window = app.get_webview_window("main").unwrap();
            let window_clone = window.clone();
            window.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    window_clone.hide().ok();
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            send_message,
            get_connection_status,
            get_history
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}