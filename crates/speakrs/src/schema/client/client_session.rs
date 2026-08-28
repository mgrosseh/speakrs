use speakrs_storage::{
    codec::{DecodeExt, Encodable},
    tree::TreeResult,
};

use crate::schema::{ClientDataStore, UserId, client::ClLensResult, server::session::SessionToken};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ClientSession {
    pub user_key: UserId,
    pub token: Option<SessionToken>, // TODO: handle cases when token expires and session is none
}

impl ClientDataStore {
    pub fn current_session(&self) -> ClLensResult<'_, Option<ClientSession>> {
        Ok(self.lens(self.side.current_session.get_single()?.decode()?))
    }

    pub fn set_current_session(&self, session: ClientSession) -> TreeResult<()> {
        self.side.current_session.insert_single(session.encode()?)?;
        Ok(())
    }
}
