use std::sync::Arc;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use sqlx::Row;
use crate::AppState;
use crate::functions::authorization::auth_from_headers;
use crate::structs::search_param::SearchParams;
use crate::structs::user_result::UserResult;

pub async fn search_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<SearchParams>,
) -> Result<Json<Vec<UserResult>>, (StatusCode, String)> {
    let claims = auth_from_headers(&headers, &state.jwt_secret)
        .ok_or((StatusCode::UNAUTHORIZED, "Требуется вход".into()))?;

    let q = params.q.trim().to_lowercase();
    if q.chars().count() < 2
        || q.chars().count() > 20
        || !q.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        return Ok(Json(vec![]));
    }
    let pattern = format!("%{q}%");
    let prefix = format!("{q}%");

    let rows = sqlx::query(
        "SELECT id, username FROM users WHERE username LIKE $1 AND id <> $4 ORDER BY CASE WHEN username = $2 THEN 0 WHEN username LIKE $3 THEN 1 ELSE 2 END, username LIMIT 20",
    )
        .bind(&pattern)
        .bind(&q)
        .bind(&prefix)
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