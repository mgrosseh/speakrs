use speakrs_storage::{
    codec::Encodable,
    tree::{TreeError, TreeResult},
};
use uuid::Uuid;

use crate::schema::DataStore;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ServerInfo {
    // TODO: better name
    /// Host system unique name
    pub name: String,
    pub uuid: Uuid,
}

impl<S> DataStore<S> {
    /// Queries the database, if initialized (server data was set) return true.
    pub fn is_init(&self) -> TreeResult<bool> {
        Ok(self.server_data.get_single()?.is_some())
    }

    /// Get server data.
    /// Run [`ServerDB::is_init()`] first to check if it's safe to get data
    pub fn server_info(&self) -> TreeResult<ServerInfo> {
        let Some(encoded) = self.server_data.get_single()? else {
            return Err(eyre::anyhow!(
                "ServerInfo not initialized. Make sure to initialialize the database before calling `get_server_data`."
            ).into());
        };

        encoded.decode().map_err(TreeError::other)
    }
    /// Set server data.
    /// Either replaces existing data with new one or initializes the database with corresponding data.
    pub fn set_server_info(&self, data: ServerInfo) -> TreeResult<()> {
        self.server_data.insert_single(data.encode()?)?;
        Ok(())
    }
}
