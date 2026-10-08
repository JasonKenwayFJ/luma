use serde::Deserialize;

#[derive(Deserialize)]
pub struct RegisterRequest {
    pub(crate) email: String,
    pub(crate) password: String,
    pub(crate) username: String,
}