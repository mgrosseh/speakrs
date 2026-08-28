use blake2::{Blake2b, Digest, digest::consts::U32};
use speakrs_storage::{
    codec::Encodable,
    tree::{TreeResult, Tx},
};

use crate::schema::{IdNotFound, ServerDataStore, User, UserId, server::user_perms::UserPerms};

type HashBytes = [u8; 32];

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct UserAuth {
    salt: String,
    #[serde(with = "serde_bytes")]
    hash: HashBytes,
}

impl UserAuth {
    pub fn from_password(password: &str) -> Self {
        let salt = uuid::Uuid::new_v4().to_string();
        Self {
            hash: Self::hash(&salt, &password),
            salt: salt,
        }
    }

    fn hash(salt: &str, password: &str) -> HashBytes {
        let mut hasher = Blake2b::<U32>::new();
        hasher.update(salt);
        hasher.update(password);
        hasher.finalize().into()
    }

    pub fn validate(&self, password: &str) -> bool {
        self.hash == Self::hash(&self.salt, password)
    }
}

impl ServerDataStore {
    pub fn user_auth(&self, id: UserId) -> TreeResult<UserAuth> {
        Ok(self
            .side
            .users_auth
            .get(id)?
            .ok_or(IdNotFound(id))?
            .decode()?)
    }

    pub fn register_user(&self, name: String, password: &str) -> TreeResult<UserId> {
        // TODO: Check if user already exists
        let data = User::new(name);
        let user_auth = UserAuth::from_password(password).encode()?;
        let user_perm = UserPerms::default().encode()?;

        Ok(
            Tx((&self.users, &self.side.users_auth, &self.side.user_perms)).transaction(
                |(tx, auth, perm)| {
                    let id = self.users.transact_add(tx, &data)?;
                    auth.insert(id, user_auth.clone())?;
                    perm.insert(id, user_perm.clone())?;
                    Ok(id)
                },
            )?,
        )
    }
}
