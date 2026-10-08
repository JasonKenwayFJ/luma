use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthResponse {
    pub(crate) token: String,
    pub user_id: String,
    pub(crate) username: String,
}