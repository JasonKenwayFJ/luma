use serde::Deserialize;

#[derive(Deserialize)]
pub struct LoginRequest {
    pub(crate) email: String,
    pub(crate) password: String,
}
