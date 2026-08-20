use crate::common::{auth::SessionToken, database::DB, schema::UserKey, table::{SerdeSingleton, TableDecl}};
use anyhow::Result;




#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ClientSession {
    pub user_key: UserKey,
    pub token: Option<SessionToken>,  // TODO: handle cases when token expires and session is none
}
pub type ClientDataTable = SerdeSingleton<ClientSession>;
pub const CLIENT_DATA_TABLE: TableDecl<ClientDataTable> = ClientDataTable::decl("client_data");

impl DB {
    /// Get client data, if present.
    /// Only intended to be used in client side code.
    pub fn get_client_data(&self) -> Result<Option<ClientSession>> {
        let tree = CLIENT_DATA_TABLE.open(&self.get_raw())?;
        Ok(tree.get_single()?)
    }
    /// Set client data.
    /// Only intended to be used in client side code.
    pub fn set_client_data(&self, data: ClientSession) -> Result<()> {
        let tree = CLIENT_DATA_TABLE.open(&self.get_raw())?;
        tree.set_single(data)?;
        Ok(())
    }

}
