use serde::Serialize;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CallSignal { pub from: String, pub(crate) room_id: String, pub(crate) signal: serde_json::Value }