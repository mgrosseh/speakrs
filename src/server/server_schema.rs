use crate::common::{auth::{SessionData, SessionToken}, database::DB, schema::UserKey, table::{SerdeTree, TableDecl}};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct UserAuthData {
    password: String,
}
impl UserAuthData {
    pub fn from_password(password: String) -> Self {
        // TODO: never store passwords in clear-text -- unless for now, I guess...
        Self {
            password
        }
    }

    pub fn validate(&self, password: String) -> bool {
        self.password == password // TODO: terrible lol
    }
}
pub type UsersAuthTable = SerdeTree<UserAuthData, UserKey>;
pub const USERS_AUTH_TABLE: TableDecl<UsersAuthTable> = UsersAuthTable::decl("user_auth");

pub type ClientSessionTable = SerdeTree<SessionData, SessionToken>;
pub const CLIENT_SESSION_TABLE: TableDecl<ClientSessionTable> = ClientSessionTable::decl("client_sessions");

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, Default)]
pub struct UserPerms {
}

pub type UserPermsTable = SerdeTree<UserPerms, UserKey>;
pub const USER_PERMS_TABLE: TableDecl<UserPermsTable> = UserPermsTable::decl("user_perms");

impl DB {
    pub(super) fn users_auth(&self) -> sled::Result<UsersAuthTable> {
        USERS_AUTH_TABLE.open(self.get_raw())
    }

    pub(super) fn client_sessions(&self) -> sled::Result<ClientSessionTable> {
        CLIENT_SESSION_TABLE.open(self.get_raw())
    }

    pub(super) fn user_perms(&self) -> sled::Result<UserPermsTable> {
        USER_PERMS_TABLE.open(self.get_raw())
    }
}
