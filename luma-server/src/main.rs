pub mod validator;
pub mod structs;
pub mod enums;
pub mod functions;

use argon2::password_hash::{SaltString, rand_core::OsRng};
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use axum::extract::ws::{Message as WsMessage, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use sqlx::postgres::{PgPoolOptions, PgRow};
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, broadcast};
use tokio::time::Instant;
use uuid::Uuid;
use crate::enums::client_frame::ClientFrame;
use crate::enums::server_frame::ServerFrame;
use crate::functions::authorization::{auth_from_headers, decode_token, is_valid_username};
use crate::functions::init_db::init_db;
use crate::functions::login::login_handler;
use crate::functions::registration::register_handler;
use crate::functions::search::search_handler;
use crate::structs::call_signal::CallSignal;
use crate::structs::claims::Claims;
use crate::structs::wire_message::WireMessage;
use crate::validator::is_valid;

const LEGACY_HISTORY_FILE: &str = "history.jsonl";
const PAGE_SIZE: i64 = 30;
const MAX_TEXT_LEN: usize = 4000;
const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;

// ---------- Сообщения ----------








fn now_iso() -> String {
    chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string()
}

// ---------- Аутентификация ----------





// ---------- HTTP: регистрация / вход / поиск ----------













#[derive(Deserialize)]
struct FcmTokenRequest {
    token: String,
    #[serde(default = "default_push_platform")]
    platform: String,
}

fn default_push_platform() -> String {
    "android".to_string()
}

async fn save_fcm_token_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<FcmTokenRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let claims = auth_from_headers(&headers, &state.jwt_secret)
        .ok_or((StatusCode::UNAUTHORIZED, "Требуется вход".into()))?;

    let token = req.token.trim();
    if token.is_empty()
        || token.len() > 4096
        || !matches!(req.platform.as_str(), "android" | "windows")
    {
        return Err((StatusCode::BAD_REQUEST, "Некорректный push-токен".into()));
    }

    sqlx::query(
        "INSERT INTO push_tokens (user_id, token, platform, updated_at)
         VALUES ($1, $2, $3, now())
         ON CONFLICT (token) DO UPDATE
         SET user_id = $1, platform = $3, updated_at = now()",
    )
    .bind(&claims.sub)
    .bind(token)
    .bind(&req.platform)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        eprintln!("save fcm token db error: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Ошибка базы данных".into(),
        )
    })?;

    Ok(StatusCode::OK)
}
// ---------- База данных: сообщения ----------



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

async fn load_room_page(
    pool: &PgPool,
    room_id: &str,
    before: Option<&str>,
    limit: i64,
) -> (Vec<WireMessage>, bool) {
    let sql = match before {
        None => format!(
            "SELECT id, room_id, user_id, author_name, avatar_url, text, attachments, {SENT_AT_EXPR}
             FROM messages WHERE room_id = $1
             ORDER BY sent_at DESC, created_at DESC LIMIT $2"
        ),
        Some(_) => format!(
            "SELECT id, room_id, user_id, author_name, avatar_url, text, attachments, {SENT_AT_EXPR}
             FROM messages WHERE room_id = $1 AND sent_at < $3::timestamptz
             ORDER BY sent_at DESC, created_at DESC LIMIT $2"
        ),
    };

    let query = sqlx::query(&sql).bind(room_id).bind(limit + 1);
    let query = match before {
        Some(ts) => query.bind(ts),
        None => query,
    };

    match query.fetch_all(pool).await {
        Ok(rows) => {
            let has_more = rows.len() as i64 > limit;
            let mut messages: Vec<WireMessage> = rows
                .iter()
                .take(limit as usize)
                .map(row_to_message)
                .collect();
            messages.reverse();
            (messages, has_more)
        }
        Err(e) => {
            eprintln!("db read error (history): {e}");
            (Vec::new(), false)
        }
    }
}

async fn load_summary(pool: &PgPool) -> Vec<WireMessage> {
    let sql = format!(
        "SELECT DISTINCT ON (room_id)
                id, room_id, user_id, author_name, avatar_url, text, attachments, {SENT_AT_EXPR}
         FROM messages ORDER BY room_id, sent_at DESC, created_at DESC"
    );
    match sqlx::query(&sql).fetch_all(pool).await {
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

    std::fs::rename(
        LEGACY_HISTORY_FILE,
        format!("{LEGACY_HISTORY_FILE}.imported"),
    )
    .ok();
    println!("Imported {imported} messages from {LEGACY_HISTORY_FILE}");
}

// ---------- WebSocket ----------

struct AppState {
    pool: PgPool,
    jwt_secret: String,
    tx: broadcast::Sender<(usize, WireMessage)>,
    call_tx: broadcast::Sender<CallSignal>,
    next_id: Mutex<usize>,
    connected_users: Mutex<HashMap<String, usize>>,
    http: reqwest::Client,
    fcm_credentials: Option<FcmServiceAccount>,
    fcm_access_token: Mutex<Option<CachedFcmToken>>,
    wns_credentials: Option<WnsCredentials>,
    wns_access_token: Mutex<Option<CachedFcmToken>>,
}

#[derive(Deserialize)]
struct FcmServiceAccount {
    project_id: String,
    client_email: String,
    private_key: String,
    token_uri: String,
}

struct WnsCredentials {
    tenant_id: String,
    client_id: String,
    client_secret: String,
}

struct CachedFcmToken {
    value: String,
    expires_at: Instant,
}

#[derive(Serialize)]
struct FcmJwtClaims<'a> {
    iss: &'a str,
    scope: &'static str,
    aud: &'a str,
    iat: usize,
    exp: usize,
}

#[derive(Deserialize)]
struct OAuthTokenResponse {
    access_token: String,
    #[serde(deserialize_with = "deserialize_expires_in")]
    expires_in: u64,
}

fn deserialize_expires_in<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum NumberOrString {
        Number(u64),
        String(String),
    }

    match NumberOrString::deserialize(deserializer)? {
        NumberOrString::Number(value) => Ok(value),
        NumberOrString::String(value) => value.parse().map_err(serde::de::Error::custom),
    }
}

async fn fcm_access_token(state: &AppState) -> Result<Option<String>, String> {
    let Some(credentials) = state.fcm_credentials.as_ref() else {
        return Ok(None);
    };

    let mut cached = state.fcm_access_token.lock().await;
    if let Some(token) = cached.as_ref() {
        if token.expires_at > Instant::now() + Duration::from_secs(60) {
            return Ok(Some(token.value.clone()));
        }
    }

    let now = chrono::Utc::now().timestamp() as usize;
    let claims = FcmJwtClaims {
        iss: &credentials.client_email,
        scope: "https://www.googleapis.com/auth/firebase.messaging",
        aud: &credentials.token_uri,
        iat: now,
        exp: now + 3600,
    };
    let key = jsonwebtoken::EncodingKey::from_rsa_pem(credentials.private_key.as_bytes())
        .map_err(|e| format!("invalid FCM service account key: {e}"))?;
    let assertion = jsonwebtoken::encode(
        &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256),
        &claims,
        &key,
    )
    .map_err(|e| format!("could not sign FCM access token: {e}"))?;

    let response = state
        .http
        .post(&credentials.token_uri)
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .body(format!(
            "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Ajwt-bearer&assertion={assertion}"
        ))
        .send()
        .await
        .map_err(|e| format!("FCM OAuth request failed: {e}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let detail = response.text().await.unwrap_or_default();
        return Err(format!("FCM OAuth returned {status}: {detail}"));
    }

    let token = response
        .json::<OAuthTokenResponse>()
        .await
        .map_err(|e| format!("invalid FCM OAuth response: {e}"))?;
    let value = token.access_token;
    *cached = Some(CachedFcmToken {
        value: value.clone(),
        expires_at: Instant::now() + Duration::from_secs(token.expires_in),
    });
    Ok(Some(value))
}

async fn wns_access_token(state: &AppState) -> Result<Option<String>, String> {
    let Some(credentials) = state.wns_credentials.as_ref() else {
        return Ok(None);
    };

    let mut cached = state.wns_access_token.lock().await;
    if let Some(token) = cached.as_ref() {
        if token.expires_at > Instant::now() + Duration::from_secs(60) {
            return Ok(Some(token.value.clone()));
        }
    }

    let form_body = {
        let mut form = url::form_urlencoded::Serializer::new(String::new());
        form.append_pair("grant_type", "client_credentials")
            .append_pair("client_id", &credentials.client_id)
            .append_pair("client_secret", &credentials.client_secret)
            .append_pair("scope", "https://wns.windows.com/.default");
        form.finish()
    };
    let endpoint = format!(
        "https://login.microsoftonline.com/{}/oauth2/v2.0/token",
        credentials.tenant_id
    );
    let response = state
        .http
        .post(endpoint)
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .body(form_body)
        .send()
        .await
        .map_err(|e| format!("WNS OAuth request failed: {e}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let detail = response.text().await.unwrap_or_default();
        return Err(format!("WNS OAuth returned {status}: {detail}"));
    }

    let token = response
        .json::<OAuthTokenResponse>()
        .await
        .map_err(|e| format!("invalid WNS OAuth response: {e}"))?;
    let value = token.access_token;
    *cached = Some(CachedFcmToken {
        value: value.clone(),
        expires_at: Instant::now() + Duration::from_secs(token.expires_in),
    });
    Ok(Some(value))
}

async fn push_recipients(
    state: &AppState,
    message: &WireMessage,
) -> Result<Vec<String>, sqlx::Error> {
    let user_ids = if let Some(participants) = message.room_id.strip_prefix("dm:") {
        participants
            .split(':')
            .filter(|id| !id.is_empty() && *id != message.user_id.as_str())
            .map(str::to_owned)
            .collect::<Vec<_>>()
    } else if matches!(message.room_id.as_str(), "general" | "dev" | "friends") {
        sqlx::query_scalar::<_, String>("SELECT id FROM users WHERE id <> $1")
            .bind(&message.user_id)
            .fetch_all(&state.pool)
            .await?
    } else {
        Vec::new()
    };

    let connected = state.connected_users.lock().await;
    Ok(user_ids
        .into_iter()
        .filter(|id| !connected.contains_key(id))
        .collect())
}

async fn send_fcm_pushes(state: Arc<AppState>, message: WireMessage) {
    let Some(credentials) = state.fcm_credentials.as_ref() else {
        return;
    };
    let recipients = match push_recipients(&state, &message).await {
        Ok(ids) if !ids.is_empty() => ids,
        Ok(_) => return,
        Err(error) => {
            eprintln!("push recipient lookup failed: {error}");
            return;
        }
    };
    let tokens = match sqlx::query_scalar::<_, String>(
        "SELECT token FROM push_tokens WHERE platform = 'android' AND user_id = ANY($1)",
    )
    .bind(&recipients)
    .fetch_all(&state.pool)
    .await
    {
        Ok(tokens) => tokens,
        Err(error) => {
            eprintln!("push token lookup failed: {error}");
            return;
        }
    };
    if tokens.is_empty() {
        return;
    }

    let access_token = match fcm_access_token(&state).await {
        Ok(Some(token)) => token,
        Ok(None) => return,
        Err(error) => {
            eprintln!("FCM authorization failed: {error}");
            return;
        }
    };
    let text = if message.text.is_empty() {
        "Вложение"
    } else {
        &message.text
    };
    let body: String = text.chars().take(180).collect();
    let endpoint = format!(
        "https://fcm.googleapis.com/v1/{}/messages:send",
        credentials.project_id
    );

    for token in tokens {
        let response = state
            .http
            .post(&endpoint)
            .bearer_auth(&access_token)
            .json(&serde_json::json!({
                "message": {
                    "token": &token,
                    "notification": {
                        "title": &message.author_name,
                        "body": &body,
                    },
                    "data": {
                        "roomId": &message.room_id,
                        "messageId": &message.id,
                    }
                }
            }))
            .send()
            .await;
        match response {
            Ok(response) if response.status().is_success() => {}
            Ok(response) => {
                let status = response.status();
                let detail = response.text().await.unwrap_or_default();
                eprintln!("FCM delivery failed ({status}): {detail}");
                if detail.contains("UNREGISTERED") {
                    sqlx::query("DELETE FROM push_tokens WHERE token = $1")
                        .bind(&token)
                        .execute(&state.pool)
                        .await
                        .ok();
                }
            }
            Err(error) => eprintln!("FCM request failed: {error}"),
        }
    }
}

fn xml_escape(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '&' => "&amp;".to_string(),
            '<' => "&lt;".to_string(),
            '>' => "&gt;".to_string(),
            '\"' => "&quot;".to_string(),
            '\'' => "&apos;".to_string(),
            _ => c.to_string(),
        })
        .collect()
}

async fn send_wns_pushes(state: Arc<AppState>, message: WireMessage) {
    if state.wns_credentials.is_none() {
        return;
    }
    let recipients = match push_recipients(&state, &message).await {
        Ok(ids) if !ids.is_empty() => ids,
        Ok(_) => return,
        Err(error) => {
            eprintln!("WNS recipient lookup failed: {error}");
            return;
        }
    };
    let channels = match sqlx::query_scalar::<_, String>(
        "SELECT token FROM push_tokens WHERE platform = 'windows' AND user_id = ANY($1)",
    )
    .bind(&recipients)
    .fetch_all(&state.pool)
    .await
    {
        Ok(channels) => channels,
        Err(error) => {
            eprintln!("WNS channel lookup failed: {error}");
            return;
        }
    };
    if channels.is_empty() {
        return;
    }

    let access_token = match wns_access_token(&state).await {
        Ok(Some(token)) => token,
        Ok(None) => return,
        Err(error) => {
            eprintln!("WNS authorization failed: {error}");
            return;
        }
    };
    let text = if message.text.is_empty() {
        "Вложение"
    } else {
        &message.text
    };
    let text: String = text.chars().take(180).collect();
    let toast = format!(
        "<toast><visual><binding template=\"ToastGeneric\"><text>{}</text><text>{}</text></binding></visual></toast>",
        xml_escape(&message.author_name),
        xml_escape(&text)
    );

    for channel in channels {
        let response = state
            .http
            .post(&channel)
            .bearer_auth(&access_token)
            .header(reqwest::header::CONTENT_TYPE, "text/xml")
            .header("X-WNS-Type", "wns/toast")
            .body(toast.clone())
            .send()
            .await;
        match response {
            Ok(response) if response.status().is_success() => {}
            Ok(response) => {
                let status = response.status();
                let detail = response.text().await.unwrap_or_default();
                eprintln!("WNS delivery failed ({status}): {detail}");
                if status == StatusCode::GONE || status == StatusCode::NOT_FOUND {
                    sqlx::query("DELETE FROM push_tokens WHERE token = $1")
                        .bind(&channel)
                        .execute(&state.pool)
                        .await
                        .ok();
                }
            }
            Err(error) => eprintln!("WNS request failed: {error}"),
        }
    }
}

#[derive(Deserialize)]
struct WsQuery {
    token: String,
}

async fn ws_handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<WsQuery>,
    ws: WebSocketUpgrade,
) -> Response {
    let claims = match decode_token(&query.token, &state.jwt_secret) {
        Some(c) => c,
        None => return (StatusCode::UNAUTHORIZED, "invalid token").into_response(),
    };
    ws.on_upgrade(move |socket| handle_connection(socket, claims.sub, claims.username, state))
}

async fn handle_connection(
    socket: WebSocket,
    user_id: String,
    username: String,
    state: Arc<AppState>,
) {
    let (mut write, mut read) = socket.split();

    let my_id = {
        let mut guard = state.next_id.lock().await;
        let id = *guard;
        *guard += 1;
        id
    };
    let mut rx = state.tx.subscribe();
    let mut call_rx = state.call_tx.subscribe();

    let summary = load_summary(&state.pool).await;
    if !summary.is_empty() {
        let json = serde_json::to_string(&ServerFrame::Summary { messages: summary }).unwrap();
        if write.send(WsMessage::Text(json)).await.is_err() {
            return;
        }
    }

    {
        let mut connected = state.connected_users.lock().await;
        *connected.entry(user_id.clone()).or_insert(0) += 1;
    }
    let mut current_room: Option<String> = None;

    loop {
        tokio::select! {
            incoming = read.next() => {
                match incoming {
                    Some(Ok(WsMessage::Text(text))) => {
                        if text.len() > MAX_FRAME_BYTES {
                            eprintln!("dropped oversized frame: {} bytes", text.len());
                            continue;
                        }

                        match serde_json::from_str::<ClientFrame>(&text) {
                            Ok(ClientFrame::Join { room_id }) => {
                                let (messages, has_more) =
                                    load_room_page(&state.pool, &room_id, None, PAGE_SIZE).await;
                                let frame = ServerFrame::History {
                                    room_id: room_id.clone(),
                                    messages,
                                    has_more,
                                    is_initial: true,
                                };
                                current_room = Some(room_id);
                                let json = serde_json::to_string(&frame).unwrap();
                                if write.send(WsMessage::Text(json)).await.is_err() {
                                    break;
                                }
                            }
                            Ok(ClientFrame::LoadMore { room_id, before_sent_at }) => {
                                let (messages, has_more) =
                                    load_room_page(&state.pool, &room_id, Some(&before_sent_at), PAGE_SIZE)
                                        .await;
                                let frame = ServerFrame::History {
                                    room_id,
                                    messages,
                                    has_more,
                                    is_initial: false,
                                };
                                let json = serde_json::to_string(&frame).unwrap();
                                if write.send(WsMessage::Text(json)).await.is_err() {
                                    break;
                                }
                            }
                            Ok(ClientFrame::Leave) => {
                                current_room = None;
                            }
                            Ok(ClientFrame::Send(mut m)) => {
                                // Личность и время отправителя ставит сервер, а не клиент.
                                m.user_id = user_id.clone();
                                m.author_name = username.clone();
                                m.sent_at = now_iso();

                                if is_valid(&m) {
                                    match save_message(&state.pool, &m).await {
                                        Ok(true) => {
                                            let _ = state.tx.send((my_id, m.clone()));
                                            let push_state = state.clone();
                                            tokio::spawn(async move {
                                                let windows_state = push_state.clone();
                                                tokio::join!(
                                                    send_fcm_pushes(push_state, m.clone()),
                                                    send_wns_pushes(windows_state, m),
                                                );
                                            });
                                        }
                                        Ok(false) => {}
                                        Err(e) => eprintln!("db write error: {e}"),
                                    }
                                }
                            }
                            Ok(ClientFrame::Signal { to, room_id, signal }) => {
                                let participants = room_id.strip_prefix("dm:").unwrap_or("");
                                if participants.split(':').any(|id| id == user_id)
                                    && participants.split(':').any(|id| id == to)
                                    && !signal.to_string().is_empty()
                                {
                                    let _ = state.call_tx.send(CallSignal { from: user_id.clone(), room_id, signal });
                                }
                            }
                            Err(e) => eprintln!("bad client frame: {e}"),
                        }
                    }
                    Some(Ok(WsMessage::Close(_))) | Some(Err(_)) | None => break,
                    _ => {}
                }
            }

            outgoing = rx.recv() => {
                match outgoing {
                    Ok((sender_id, m)) => {
                        if sender_id == my_id {
                            continue;
                        }
                        let frame = if current_room.as_deref() == Some(m.room_id.as_str()) {
                            ServerFrame::Message(m)
                        } else {
                            ServerFrame::Preview(m)
                        };
                        let json = serde_json::to_string(&frame).unwrap();
                        if write.send(WsMessage::Text(json)).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => {}
                }
            }

            call = call_rx.recv() => {
                if let Ok(call) = call {
                    if call.from != user_id {
                        let target = call.room_id.strip_prefix("dm:").unwrap_or("");
                        if target.split(':').any(|id| id == user_id) {
                            let json = serde_json::json!({"type":"callSignal", "from":call.from, "roomId":call.room_id, "signal":call.signal}).to_string();
                            if write.send(WsMessage::Text(json)).await.is_err() { break; }
                        }
                    }
                }
            }
        }
    }

    let mut connected = state.connected_users.lock().await;
    if let Some(count) = connected.get_mut(&user_id) {
        *count -= 1;
        if *count == 0 {
            connected.remove(&user_id);
        }
    }
}

// ---------- main ----------

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL is not set (create luma-server/.env)");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(15))
        .connect(&url)
        .await
        .expect("cannot connect to database");

    init_db(&pool).await;
    import_legacy_history(&pool).await;
    println!("DB ready (Postgres)");

    let jwt_secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| {
        eprintln!("WARNING: JWT_SECRET is not set — using an insecure default. Set it in .env.");
        "dev-insecure-secret-change-me".to_string()
    });

    let (tx, _rx) = broadcast::channel::<(usize, WireMessage)>(200);
    let (call_tx, _call_rx) = broadcast::channel::<CallSignal>(100);

    let fcm_credentials =
        std::env::var("FCM_SERVICE_ACCOUNT_JSON").ok().and_then(
            |value| match serde_json::from_str::<FcmServiceAccount>(&value) {
                Ok(credentials) => Some(credentials),
                Err(error) => {
                    eprintln!("FCM_SERVICE_ACCOUNT_JSON is invalid: {error}");
                    None
                }
            },
        );
    if fcm_credentials.is_none() {
        eprintln!("FCM pushes are disabled: set FCM_SERVICE_ACCOUNT_JSON on the server");
    }
    let wns_credentials = match (
        std::env::var("WNS_TENANT_ID"),
        std::env::var("WNS_CLIENT_ID"),
        std::env::var("WNS_CLIENT_SECRET"),
    ) {
        (Ok(tenant_id), Ok(client_id), Ok(client_secret)) => Some(WnsCredentials {
            tenant_id,
            client_id,
            client_secret,
        }),
        _ => {
            eprintln!(
                "WNS pushes are disabled: configure WNS_TENANT_ID, WNS_CLIENT_ID, and WNS_CLIENT_SECRET"
            );
            None
        }
    };

    let state = Arc::new(AppState {
        pool,
        jwt_secret,
        tx,
        call_tx,
        next_id: Mutex::new(0),
        connected_users: Mutex::new(HashMap::new()),
        http: reqwest::Client::new(),
        fcm_credentials,
        fcm_access_token: Mutex::new(None),
        wns_credentials,
        wns_access_token: Mutex::new(None),
    });

    let app = Router::new()
        .route("/api/register", post(register_handler))
        .route("/api/login", post(login_handler))
        .route("/api/users/search", get(search_handler))
        .route("/api/push-token", post(save_fcm_token_handler))
        .route("/ws", get(ws_handler))
        .route("/api/health", get(|| async { "OK" }))
        .route("/api/ping", get(|| async { "Pong!" }))
        .with_state(state);

    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    println!("Luma server running on {addr} (HTTP + WS at /ws)");
    axum::serve(listener, app).await.unwrap();
}
