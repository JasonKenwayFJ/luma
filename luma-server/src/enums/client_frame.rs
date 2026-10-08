use serde::Deserialize;
use crate::structs::wire_message::WireMessage;

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ClientFrame {
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
    Signal { to: String, room_id: String, signal: serde_json::Value },
}