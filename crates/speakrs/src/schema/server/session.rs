// TODO: currently data flow is not tunnelled, so the auth system is kind of just for future rn.

use speakrs_storage::{key::UuidKey, pagination::Edge, tree::TreeResult};

use crate::{
    common::lens::Lens,
    schema::{
        IdNotFound, LensResult, UserId,
        server::{ServerDataStore, SvLensResult},
        user::User,
    },
};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Session {
    pub user: UserId,
}
impl Session {
    pub fn new(user: UserId) -> Self {
        Self { user }
    }
}

pub type SessionToken = UuidKey<Session>;

impl ServerDataStore {
    pub fn session(&self, id: SessionToken) -> SvLensResult<'_, Edge<Session>> {
        Ok(self.lens(self.side.sessions.get_edge(id)?.ok_or(IdNotFound(id))?))
    }

    pub fn add_session(&self, data: Session) -> TreeResult<SessionToken> {
        self.side.sessions.add(data)
    }
}

impl<'db, S> Lens<'db, Edge<Session>, S> {
    #[allow(unused)] // TODO
    pub fn user(&self) -> LensResult<'db, Edge<User>, S> {
        self.store.user(self.user)
    }
}
