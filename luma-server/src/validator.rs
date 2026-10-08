use crate::{WireMessage, MAX_TEXT_LEN};

pub fn is_valid(m: &WireMessage) -> bool {
    !m.id.is_empty()
        && !m.user_id.is_empty()
        && !m.room_id.is_empty()
        && m.author_name.chars().count() <= 50
        && m.text.chars().count() <= MAX_TEXT_LEN
        && (!m.text.is_empty() || m.attachments.is_some())
}