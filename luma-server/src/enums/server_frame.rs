use serde::Serialize;
use crate::structs::wire_message::WireMessage;

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ServerFrame {
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
