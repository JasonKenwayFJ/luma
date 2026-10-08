use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct WireMessage {
    pub(crate) id: String,
    pub(crate) user_id: String,
    pub(crate) room_id: String,
    pub(crate) author_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) avatar_url: Option<String>,
    #[serde(default)]
    pub(crate) text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) attachments: Option<serde_json::Value>,
    pub(crate) sent_at: String,
}