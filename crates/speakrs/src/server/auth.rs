use speakrs_storage::codec::{Decodable, Encodable};

use crate::common::{
    auth::{SessionData, SessionToken},
    database::DB,
    rpc::{ServiceError, ServiceResult},
    schema::{ChannelKey, UserData, UserKey},
};

use super::server_schema::{UserAuthData, UserPerms};

#[derive(thiserror::Error, Debug, serde::Deserialize, serde::Serialize)]
pub enum AuthError {
    #[error("User does not exist.")]
    NoSuchUser,
    #[error("Password for the user is incorrect.")]
    IncorrectPassword,
    #[error("Auth token is invalid (or expired).")]
    InvalidToken,
    #[error("User has insufficient permissions for this action.")]
    InsufficientPerms,
}

pub fn authenticate_session(
    db: DB,
    user: UserKey,
    password: String,
) -> ServiceResult<SessionToken> {
    let auth_data = db
        .users_auth()?
        .get(user)?
        .ok_or(ServiceError::Auth(AuthError::NoSuchUser))?
        .decode()?;
    if !auth_data.validate(&password) {
        return Err(ServiceError::Auth(AuthError::IncorrectPassword));
    }
    register_token(db, user)
}

pub fn register_user(db: DB, data: UserData, password: String) -> ServiceResult<UserKey> {
    // TODO: we might want to check if user already exists
    // TODO: this being two steps introduces failure points!
    let key = UserKey::new_now();
    db.users()?.insert(key, data.encode()?)?;
    db.users_auth()?
        .insert(key, UserAuthData::from_password(&password).encode()?)?;
    db.user_perms()?
        .insert(key, UserPerms::default().encode()?)?;
    Ok(key)
}

fn register_token(db: DB, user: UserKey) -> ServiceResult<SessionToken> {
    let token = SessionToken::new_now();
    db.client_sessions()?
        .insert(token, SessionData::new(user).encode()?)?;
    Ok(token)
}

fn validate_token(db: DB, token: SessionToken) -> ServiceResult<SessionData> {
    if let Some(data) = db.client_sessions()?.get(token)? {
        Ok(data.decode()?)
    } else {
        Err(ServiceError::Auth(AuthError::InvalidToken))
    }
}

pub fn validate_session(db: DB, session: SessionToken) -> ServiceResult<bool> {
    Ok(db.client_sessions()?.get(session)?.is_some())
}

pub fn permission_guard(db: DB, token: SessionToken, perms: &[Permissions]) -> ServiceResult<()> {
    let data = validate_token(db.clone(), token)?;
    if !Permissions::check(perms, db, data.user)? {
        Err(ServiceError::Auth(AuthError::InsufficientPerms))
    } else {
        Ok(())
    }
}

#[allow(unused)] // TODO
pub enum Permissions {
    CanCreateChannel,
    CanWriteMessageIn(ChannelKey),
    CanReadMessageIn(ChannelKey),
    CanSeeUser(UserKey),
}

impl Permissions {
    /// TODO
    pub fn is_allowed(&self, _perms: &UserPerms) -> bool {
        true // TODO poll database instead of allowing everything to everyone
    }
    /// TODO
    pub fn check(perms: &[Permissions], db: DB, user: UserKey) -> ServiceResult<bool> {
        Ok(db
            .user_perms()?
            .get(user)?
            .map(Decodable::decode)
            .transpose()?
            .map_or(false, |user_perms| {
                perms.iter().all(|p| p.is_allowed(&user_perms))
            }))
    }
}
