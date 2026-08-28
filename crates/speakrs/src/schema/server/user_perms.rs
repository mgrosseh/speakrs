use speakrs_storage::tree::TreeResult;

use crate::schema::{IdNotFound, ServerDataStore, UserId};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, Default)]
pub struct UserPerms {}

impl ServerDataStore {
    pub fn user_perms(&self, id: UserId) -> TreeResult<UserPerms> {
        Ok(self
            .side
            .user_perms
            .get(id)?
            .ok_or(IdNotFound(id))?
            .decode()?)
    }
}
