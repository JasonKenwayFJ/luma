use std::sync::Arc;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use sqlx::Row;
use crate::AppState;
use crate::functions::authorization::{make_token, verify_password};
use crate::structs::auth_response::AuthResponse;
use crate::structs::login_requst::LoginRequest;

pub async fn login_handler(
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
