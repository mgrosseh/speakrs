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
    let auth_data = db.users_auth()?.get(user)?.ok_or(ServiceError::AuthError(AuthError::NoSuchUser))?;
    if !auth_data.validate(&password) {
        return Err(ServiceError::AuthError(AuthError::IncorrectPassword));
    }
    register_token(db, user)
}

pub fn register_user(db: DB, data: UserData, password: String) -> ServiceResult<UserKey> {
    // TODO: we might want to check if user already exists
    // TODO: this being two steps introduces failure points!
    let key = db
        .users()?
        .insert(data)
        .map_err(|e| Into::<ServiceError>::into(e))?;
    db.users_auth()?
        .set(key, UserAuthData::from_password(&password))
        .map_err(|e| Into::<ServiceError>::into(e))?;
    db.user_perms()?.set(key, UserPerms::default())?;
    Ok(key)
}

fn register_token(db: DB, user: UserKey) -> ServiceResult<SessionToken> {
    let token = SessionToken::new_now();
    db.client_sessions()?.set(token, SessionData::new(user))?;
    Ok(token)
}

fn validate_token(db: DB, token: SessionToken) -> ServiceResult<SessionData> {
    if let Some(data) = db.client_sessions()?.get(token)? {
        Ok(data)
    } else {
        Err(ServiceError::AuthError(AuthError::InvalidToken))
    }
}

pub fn permission_guard(db: DB, token: SessionToken, perms: &[Permissions]) -> ServiceResult<()> {
    let data = validate_token(db.clone(), token)?;
    if !Permissions::check(perms, db, data.user)? {
        Err(ServiceError::AuthError(AuthError::InsufficientPerms))
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
        Ok(db.user_perms()?.get(user)?.map_or(false, |user_perms| {
            perms.iter().all(|p| p.is_allowed(&user_perms))
        }))
    }
}
