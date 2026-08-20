use std::collections::HashMap;

use blake2::{Blake2b, Digest, digest::consts::U32};
use uuid::Uuid;

use crate::common::{
    auth::{SessionData, SessionToken},
    database::{DB, DBCommonDump},
    schema::UserKey,
    table::{OpenResult, SerdeTree, TableDecl},
};

type HashStorage = [u8; 32];
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct UserAuthData {
    salt: String,
    #[serde(with = "serde_bytes")]
    hash: HashStorage,
}
impl UserAuthData {
    pub fn from_password(password: &str) -> Self {
        let salt = Uuid::new_v4().to_string();
        Self {
            hash: Self::hash(&salt, &password),
            salt: salt,
        }
    }

    fn hash(salt: &str, password: &str) -> HashStorage {
        let mut hasher = Blake2b::<U32>::new();
        hasher.update(salt);
        hasher.update(password);
        hasher.finalize().into()
    }

    pub fn validate(&self, password: &str) -> bool {
        self.hash == Self::hash(&self.salt, password)
    }
}
pub type UsersAuthTable = SerdeTree<UserAuthData, UserKey>;
pub const USERS_AUTH_TABLE: TableDecl<UsersAuthTable> = UsersAuthTable::decl("user_auth");

pub type ClientSessionTable = SerdeTree<SessionData, SessionToken>;
pub const CLIENT_SESSION_TABLE: TableDecl<ClientSessionTable> =
    ClientSessionTable::decl("client_sessions");

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, Default)]
pub struct UserPerms {}

pub type UserPermsTable = SerdeTree<UserPerms, UserKey>;
pub const USER_PERMS_TABLE: TableDecl<UserPermsTable> = UserPermsTable::decl("user_perms");

impl DB {
    pub(super) fn users_auth(&self) -> OpenResult<UsersAuthTable> {
        USERS_AUTH_TABLE.open(self.get_raw())
    }

    pub(super) fn client_sessions(&self) -> OpenResult<ClientSessionTable> {
        CLIENT_SESSION_TABLE.open(self.get_raw())
    }

    pub(super) fn user_perms(&self) -> OpenResult<UserPermsTable> {
        USER_PERMS_TABLE.open(self.get_raw())
    }
}

pub trait ServerDump {
    fn dump(&self) -> anyhow::Result<DBDump>;
}

impl ServerDump for DB {
    fn dump(&self) -> anyhow::Result<DBDump> {
        let mut client_sessions = HashMap::new();
        for result in self.client_sessions()?.iter() {
            let (key, value) = result?;
            client_sessions.insert(key, value);
        }
        let mut users_auth = HashMap::new();
        for result in self.users_auth()?.iter() {
            let (key, value) = result?;
            users_auth.insert(key, value);
        }
        let mut user_perms = HashMap::new();
        for result in self.user_perms()?.iter() {
            let (key, value) = result?;
            user_perms.insert(key, value);
        }
        let common = self.dump_shared()?;
        Ok(DBDump { client_sessions, users_auth, user_perms, common })
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct DBDump {
    client_sessions: HashMap<SessionToken, SessionData>,
    users_auth: HashMap<UserKey, UserAuthData>,
    user_perms: HashMap<UserKey, UserPerms>,
    common: DBCommonDump,
}
