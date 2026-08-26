// TODO: currently data flow is not tunnelled, so the auth system is kind of just for future rn.

use speakrs_storage::key::UuidKey;

use crate::common::schema::UserKey;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionData {
    pub user: UserKey,
}
impl SessionData {
    pub fn new(user: UserKey) -> Self {
        Self { user }
    }
}
pub type SessionToken = UuidKey<SessionData>;
