#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use futures_util::{SinkExt, StreamExt};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Emitter, Manager};
use tauri_plugin_notification::NotificationExt;
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};

struct WsState {
    tx: mpsc::UnboundedSender<String>,
}

#[tauri::command]
fn send_message(text: String, state: tauri::State<WsState>) -> Result<(), String> {
    state.tx.send(text).map_err(|e| e.to_string())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let app_handle = app.handle().clone();

            // канал: команды из React -> задача с WS-соединением
            let (tx, mut rx) = mpsc::unbounded_channel::<String>();
            app.manage(WsState { tx });

            tauri::async_runtime::spawn(async move {
                let url = "ws://127.0.0.1:8080";
                let (ws_stream, _) = match connect_async(url).await {
                    Ok(v) => v,
                    Err(e) => {
                        eprintln!("WS connect error: {e}");
                        return;
                    }
                };
                let (mut write, mut read) = ws_stream.split();

                let write_task = tokio::spawn(async move {
                    while let Some(msg) = rx.recv().await {
                        if write.send(Message::Text(msg)).await.is_err() {
                            break;
                        }
                    }
                });

                while let Some(msg) = read.next().await {
                    match msg {
                        Ok(Message::Text(text)) => {
                            let window = app_handle.get_webview_window("main");
                            let is_visible = window
                                .as_ref()
                                .map(|w| w.is_visible().unwrap_or(false))
                                .unwrap_or(false);

                            app_handle.emit("ws-message", text.clone()).ok();

                            if !is_visible {
                                app_handle
                                    .notification()
                                    .builder()
                                    .title("Luma")
                                    .body(&text)
                                    .show()
                                    .ok();
                            }
                        }
                        Ok(Message::Close(_)) | Err(_) => break,
                        _ => {}
                    }
                }
                write_task.abort();
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
        .invoke_handler(tauri::generate_handler![send_message])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}