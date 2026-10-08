use std::sync::Arc;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use uuid::Uuid;
use crate::AppState;
use crate::functions::authorization::{hash_password, is_valid_username, make_token};
use crate::structs::auth_response::AuthResponse;
use crate::structs::register_request::RegisterRequest;

pub async fn register_handler(
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