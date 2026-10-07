#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Emitter, Manager};
use tauri_plugin_notification::NotificationExt;
use tokio::sync::{mpsc, Notify};
use tokio_tungstenite::{connect_async, tungstenite::Message};

const API_BASE: &str = "https://luma-otjt.onrender.com";

struct WsState {
    tx: mpsc::UnboundedSender<String>,
    connected: Arc<AtomicBool>,
    summary: Arc<Mutex<Option<String>>>,
    token: Arc<Mutex<Option<String>>>,
    notify: Arc<Notify>,
}

#[derive(Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct AuthResult {
    token: String,
    user_id: String,
    username: String,
}

#[derive(Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct UserResult {
    user_id: String,
    username: String,
}
#[tauri::command]
async fn save_fcm_token(token: String, auth_token: String) -> Result<(), String> {
    let client = reqwest::Client::new();
    let res = client
        .post(format!("{API_BASE}/api/push-token"))
        .bearer_auth(auth_token)
        .json(&serde_json::json!({ "token": token }))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err("Не удалось сохранить push-токен".into());
    }
    Ok(())
}
#[tauri::command]
async fn register(email: String, password: String, username: String) -> Result<AuthResult, String> {
    let client = reqwest::Client::new();
    let res = client
        .post(format!("{API_BASE}/api/register"))
        .json(&serde_json::json!({ "email": email, "password": password, "username": username }))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(res
            .text()
            .await
            .unwrap_or_else(|_| "Ошибка регистрации".into()));
    }
    res.json::<AuthResult>().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn login(email: String, password: String) -> Result<AuthResult, String> {
    let client = reqwest::Client::new();
    let res = client
        .post(format!("{API_BASE}/api/login"))
        .json(&serde_json::json!({ "email": email, "password": password }))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(res.text().await.unwrap_or_else(|_| "Ошибка входа".into()));
    }
    res.json::<AuthResult>().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn search_users(query: String, token: String) -> Result<Vec<UserResult>, String> {
    let client = reqwest::Client::new();
    let res = client
        .get(format!("{API_BASE}/api/users/search"))
        .query(&[("q", query)])
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err("Ошибка поиска".into());
    }
    res.json::<Vec<UserResult>>()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn connect_ws(token: String, state: tauri::State<WsState>) {
    *state.token.lock().unwrap() = Some(token);
    state.notify.notify_one();
}

#[tauri::command]
fn disconnect_ws(state: tauri::State<WsState>) {
    *state.token.lock().unwrap() = None;
    state.notify.notify_one();
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
    state.summary.lock().unwrap().clone()
}

fn frame_type(raw: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(raw)
        .ok()
        .and_then(|v| {
            v.get("type")
                .and_then(|t| t.as_str().map(|s| s.to_string()))
        })
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
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let app_handle = app.handle().clone();

            let (tx, mut rx) = mpsc::unbounded_channel::<String>();
            let connected = Arc::new(AtomicBool::new(false));
            let summary: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
            let token: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
            let notify = Arc::new(Notify::new());

            app.manage(WsState {
                tx,
                connected: connected.clone(),
                summary: summary.clone(),
                token: token.clone(),
                notify: notify.clone(),
            });

            tauri::async_runtime::spawn(async move {
                loop {
                    let current_token = token.lock().unwrap().clone();
                    let active_token = match current_token {
                        Some(t) => t,
                        None => {
                            notify.notified().await;
                            continue;
                        }
                    };

                    let url = format!("wss://luma-otjt.onrender.com/ws?token={active_token}");
                    let mut reconnect_immediately = false;

                    match connect_async(&url).await {
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
                                                match frame_type(&text).as_deref() {
                                                    Some("summary") => {
                                                        *summary.lock().unwrap() = Some(text.clone());
                                                        app_handle.emit("ws-frame", text).ok();
                                                    }
                                                    Some("message") | Some("preview") => {
                                                        let is_visible = app_handle
                                                            .get_webview_window("main")
                                                            .map(|w| w.is_visible().unwrap_or(false))
                                                            .unwrap_or(false);

                                                        app_handle.emit("ws-frame", text.clone()).ok();

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
                                                    _ => {
                                                        app_handle.emit("ws-frame", text).ok();
                                                    }
                                                }
                                            }
                                            Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                                            _ => {}
                                        }
                                    }

                                    // Токен сменился (перелогин) или разлогинились — рвём это
                                    // соединение; на следующем витке либо переподключимся с
                                    // новым токеном, либо будем ждать, если токена нет.
                                    _ = notify.notified() => {
                                        reconnect_immediately = true;
                                        break;
                                    }
                                }
                            }

                            connected.store(false, Ordering::SeqCst);
                            app_handle.emit("ws-status", false).ok();
                            println!("WS disconnected");
                        }
                        Err(e) => eprintln!("WS connect error: {e}"),
                    }

                    if !reconnect_immediately {
                        tokio::time::sleep(Duration::from_secs(2)).await;
                    }
                }
            });

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
            get_history,
            register,
            login,
            search_users,
            connect_ws,
            disconnect_ws,
            save_fcm_token
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
