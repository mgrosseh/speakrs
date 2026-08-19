// TODO: currently data flow is not tunnelled, so the auth system is kind of just for future rn.

use super::{key::UuidKey, schema::UserKey};

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
