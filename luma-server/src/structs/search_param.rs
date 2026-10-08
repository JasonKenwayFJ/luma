use serde::Deserialize;

#[derive(Deserialize)]
pub struct SearchParams {
    pub(crate) q: String,
}
