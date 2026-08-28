use speakrs_storage::tree::TreeError;

use crate::{
    common::rpc::{ServiceError, ServiceResult},
    schema::{
        ServerDataStore, SessionToken,
        channel::ChannelId,
        server::{session::Session, user_perms::UserPerms},
        user::UserId,
    },
};

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
    db: &ServerDataStore,
    user: UserId,
    password: String,
) -> ServiceResult<SessionToken> {
    let auth_data = db.user_auth(user)?;
    if !auth_data.validate(&password) {
        return Err(ServiceError::Auth(AuthError::IncorrectPassword));
    }
    Ok(db.add_session(Session::new(user))?)
}

fn validate_token(db: &ServerDataStore, token: SessionToken) -> ServiceResult<Session> {
    let session = db.session(token).map_err(|err| match err {
        TreeError::Storage(err) => ServiceError::from(err),
        TreeError::Other(_) => ServiceError::Auth(AuthError::InvalidToken),
    })?;
    Ok(session.focus.node)
}

pub fn validate_session(db: &ServerDataStore, token: SessionToken) -> ServiceResult<bool> {
    match validate_token(db, token) {
        Ok(_) => Ok(true),
        Err(ServiceError::Auth(_)) => Ok(false),
        Err(err) => Err(err),
    }
}

pub fn permission_guard(
    db: &ServerDataStore,
    token: SessionToken,
    perms: &[Permissions],
) -> ServiceResult<()> {
    let data = validate_token(db, token)?;
    if !Permissions::check(db, perms, data.user)? {
        Err(ServiceError::Auth(AuthError::InsufficientPerms))
    } else {
        Ok(())
    }
}

#[allow(unused)] // TODO
pub enum Permissions {
    CanCreateChannel,
    CanWriteMessageIn(ChannelId),
    CanReadMessageIn(ChannelId),
    CanSeeUser(UserId),
}

impl Permissions {
    /// TODO
    pub fn is_allowed(&self, _perms: &UserPerms) -> bool {
        true // TODO poll database instead of allowing everything to everyone
    }
    /// TODO
    pub fn check(db: &ServerDataStore, perms: &[Permissions], user: UserId) -> ServiceResult<bool> {
        let user_perms = db.user_perms(user)?;
        Ok(perms.iter().all(|p| p.is_allowed(&user_perms)))
    }
}
