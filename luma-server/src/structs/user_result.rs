use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserResult {
    pub(crate) user_id: String,
    pub(crate) username: String,
}