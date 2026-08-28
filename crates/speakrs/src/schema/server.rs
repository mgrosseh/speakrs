pub mod session;
pub mod user_auth;
pub mod user_perms;

use speakrs_storage::{
    codec::SerdeJsonCodec,
    table::{OneToOne, Primary},
    tree::{TreeResult, TypedTree},
};

use crate::{
    common::lens::Lens,
    schema::{
        DataStore, SideData, SidedData, UserId,
        server::{session::Session, user_auth::UserAuth, user_perms::UserPerms},
    },
};

pub type ServerDataStore = DataStore<ServerOnlyData>;
pub(self) type SvLens<'db, T> = Lens<'db, T, ServerOnlyData>;
pub(self) type SvLensResult<'db, T> = TreeResult<SvLens<'db, T>>;

#[derive(Clone)]
pub struct ServerOnlyData {
    pub(self) sessions: Primary<Session, SerdeJsonCodec>,
    pub(self) users_auth: OneToOne<UserId, UserAuth, SerdeJsonCodec>,
    pub(self) user_perms: OneToOne<UserId, UserPerms, SerdeJsonCodec>,
}

impl std::fmt::Debug for ServerOnlyData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerOnlyData").finish()
    }
}

impl SideData for ServerOnlyData {
    fn as_enum(&self) -> SidedData<'_> {
        SidedData::Server(self)
    }

    fn new(db: &sled::Db) -> sled::Result<Self> {
        Ok(ServerOnlyData {
            sessions: TypedTree::open(&db, "sessions")?,
            users_auth: TypedTree::open(&db, "users_auth")?,
            user_perms: TypedTree::open(&db, "users_perm")?,
        })
    }
}
