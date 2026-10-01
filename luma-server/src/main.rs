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

const LEGACY_HISTORY_FILE: &str = "history.jsonl";
const PAGE_SIZE: i64 = 30;
const MAX_TEXT_LEN: usize = 4000;
const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;

// ---------- Сообщения ----------

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

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum ClientFrame {
    Join {
        #[serde(rename = "roomId")]
        room_id: String,
    },
    LoadMore {
        #[serde(rename = "roomId")]
        room_id: String,
        #[serde(rename = "beforeSentAt")]
        before_sent_at: String,
    },
    Leave,
    Send(WireMessage),
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum ServerFrame {
    Summary {
        messages: Vec<WireMessage>,
    },
    History {
        #[serde(rename = "roomId")]
        room_id: String,
        messages: Vec<WireMessage>,
        #[serde(rename = "hasMore")]
        has_more: bool,
        #[serde(rename = "isInitial")]
        is_initial: bool,
    },
    Message(WireMessage),
    Preview(WireMessage),
}

fn is_valid(m: &WireMessage) -> bool {
    !m.id.is_empty()
        && !m.user_id.is_empty()
        && !m.room_id.is_empty()
        && m.author_name.chars().count() <= 50
        && m.text.chars().count() <= MAX_TEXT_LEN
        && (!m.text.is_empty() || m.attachments.is_some())
}

fn now_iso() -> String {
    chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string()
}

// ---------- Аутентификация ----------

#[derive(Serialize, Deserialize)]
struct Claims {
    sub: String, // user_id
    username: String,
    exp: usize,
}

fn make_token(
    secret: &str,
    user_id: &str,
    username: &str,
) -> Result<String, jsonwebtoken::errors::Error> {
    let exp = (chrono::Utc::now() + chrono::Duration::days(30)).timestamp() as usize;
    let claims = Claims {
        sub: user_id.to_string(),
        username: username.to_string(),
        exp,
    };
    jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(secret.as_bytes()),
    )
}

fn decode_token(token: &str, secret: &str) -> Option<Claims> {
    jsonwebtoken::decode::<Claims>(
        token,
        &jsonwebtoken::DecodingKey::from_secret(secret.as_bytes()),
        &jsonwebtoken::Validation::default(),
    )
    .ok()
    .map(|d| d.claims)
}

// Достаёт токен из заголовка "Authorization: Bearer <token>" — используется
// эндпоинтом поиска, куда клиент стучится обычным HTTP-запросом.
fn auth_from_headers(headers: &HeaderMap, secret: &str) -> Option<Claims> {
    let value = headers
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?;
    let token = value.strip_prefix("Bearer ")?;
    decode_token(token, secret)
}

fn hash_password(password: &str) -> Result<String, argon2::password_hash::Error> {
    let salt = SaltString::generate(&mut OsRng);
    Ok(Argon2::default()
        .hash_password(password.as_bytes(), &salt)?
        .to_string())
}

fn verify_password(password: &str, hash: &str) -> bool {
    match PasswordHash::new(hash) {
        Ok(parsed) => Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => false,
    }
}

fn is_valid_username(u: &str) -> bool {
    u.len() >= 3
        && u.len() <= 20
        && u.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

// ---------- HTTP: регистрация / вход / поиск ----------

#[derive(Deserialize)]
struct RegisterRequest {
    email: String,
    password: String,
    username: String,
}

#[derive(Deserialize)]
struct LoginRequest {
    email: String,
    password: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthResponse {
    token: String,
    user_id: String,
    username: String,
}

#[derive(Deserialize)]
struct SearchParams {
    q: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UserResult {
    user_id: String,
    username: String,
}

async fn register_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RegisterRequest>,
) -> Result<Json<AuthResponse>, (StatusCode, String)> {
    let email = req.email.trim().to_lowercase();
    let username = req.username.trim().to_lowercase();

    if !email.contains('@') || email.len() > 254 {
        return Err((StatusCode::BAD_REQUEST, "Некорректный email".into()));
    }
    if !is_valid_username(&username) {
        return Err((
            StatusCode::BAD_REQUEST,
            "Юзернейм: 3-20 символов, a-z, 0-9, _".into(),
        ));
    }
    if req.password.len() < 6 {
        return Err((StatusCode::BAD_REQUEST, "Пароль минимум 6 символов".into()));
    }

    let password_hash = hash_password(&req.password).map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Ошибка хеширования".into(),
        )
    })?;
    let user_id = Uuid::new_v4().to_string();

    let result = sqlx::query(
        "INSERT INTO users (id, email, username, password_hash) VALUES ($1, $2, $3, $4)",
    )
    .bind(&user_id)
    .bind(&email)
    .bind(&username)
    .bind(&password_hash)
    .execute(&state.pool)
    .await;

    if let Err(e) = result {
        if let Some(db_err) = e.as_database_error() {
            if db_err.code().as_deref() == Some("23505") {
                return Err((
                    StatusCode::CONFLICT,
                    "Такой email или юзернейм уже занят".into(),
                ));
            }
        }
        eprintln!("register db error: {e}");
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            "Ошибка базы данных".into(),
        ));
    }

    let token = make_token(&state.jwt_secret, &user_id, &username)
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Ошибка токена".into()))?;

    Ok(Json(AuthResponse {
        token,
        user_id,
        username,
    }))
}

async fn login_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<AuthResponse>, (StatusCode, String)> {
    let email = req.email.trim().to_lowercase();

    let row = sqlx::query("SELECT id, username, password_hash FROM users WHERE email = $1")
        .bind(&email)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| {
            eprintln!("login db error: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Ошибка базы данных".into(),
            )
        })?
        .ok_or((StatusCode::UNAUTHORIZED, "Неверный email или пароль".into()))?;

    let hash: String = row.get("password_hash");
    if !verify_password(&req.password, &hash) {
        return Err((StatusCode::UNAUTHORIZED, "Неверный email или пароль".into()));
    }

    let user_id: String = row.get("id");
    let username: String = row.get("username");
    let token = make_token(&state.jwt_secret, &user_id, &username)
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Ошибка токена".into()))?;

    Ok(Json(AuthResponse {
        token,
        user_id,
        username,
    }))
}

async fn search_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<SearchParams>,
) -> Result<Json<Vec<UserResult>>, (StatusCode, String)> {
    let claims = auth_from_headers(&headers, &state.jwt_secret)
        .ok_or((StatusCode::UNAUTHORIZED, "Требуется вход".into()))?;

    let q = params.q.trim().to_lowercase();
    if q.len() < 2 {
        return Ok(Json(vec![]));
    }
    let pattern = format!("%{q}%");

    let rows = sqlx::query(
        "SELECT id, username FROM users WHERE username ILIKE $1 AND id <> $2 ORDER BY username LIMIT 20",
    )
        .bind(&pattern)
        .bind(&claims.sub)
        .fetch_all(&state.pool)
        .await
        .map_err(|e| {
            eprintln!("search db error: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "Ошибка базы данных".into())
        })?;

    let results = rows
        .iter()
        .map(|r| UserResult {
            user_id: r.get("id"),
            username: r.get("username"),
        })
        .collect();

    Ok(Json(results))
}
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

async fn init_db(pool: &PgPool) {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS users (
            id            TEXT PRIMARY KEY,
            email         TEXT NOT NULL UNIQUE,
            username      TEXT NOT NULL UNIQUE,
            password_hash TEXT NOT NULL,
            created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
        )",
    )
    .execute(pool)
    .await
    .expect("cannot create users table");
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS fcm_tokens (
        user_id    TEXT PRIMARY KEY,
        token      TEXT NOT NULL,
        updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
    )",
    )
    .execute(pool)
    .await
    .expect("cannot create fcm_tokens table");
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS push_tokens (
            token      TEXT PRIMARY KEY,
            user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            platform   TEXT NOT NULL CHECK (platform IN ('android', 'windows')),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
        )",
    )
    .execute(pool)
    .await
    .expect("cannot create push_tokens table");
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_push_tokens_user_platform ON push_tokens (user_id, platform)")
        .execute(pool)
        .await
        .expect("cannot create push token index");
    sqlx::query(
        "INSERT INTO push_tokens (user_id, token, platform, updated_at)
         SELECT user_id, token, 'android', updated_at FROM fcm_tokens WHERE btrim(token) <> ''
         ON CONFLICT (token) DO UPDATE SET user_id = EXCLUDED.user_id, updated_at = EXCLUDED.updated_at",
    )
        .execute(pool)
        .await
        .expect("cannot migrate fcm tokens");
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
    .execute(pool)
    .await
    .expect("cannot create messages table");

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_messages_room_sent ON messages (room_id, sent_at)")
        .execute(pool)
        .await
        .expect("cannot create index");
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
        .with_state(state);

    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    println!("Luma server running on {addr} (HTTP + WS at /ws)");
    axum::serve(listener, app).await.unwrap();
}
