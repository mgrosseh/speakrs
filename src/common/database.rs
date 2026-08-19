use std::path::Path;

use anyhow::{Context, Result};
use tracing::info;
use uuid::Uuid;

use crate::{
    client::ClientConfig,
    common::{schema::*, table::OpenResult},
    server::ServerConfig,
};

#[derive(Debug, Clone)]
pub struct DB {
    db: sled::Db,
}

impl DB {
    #[cfg(test)]
    pub fn mock() -> Self {
        let db = sled::Config::new().temporary(true).open().expect("open");
        Self { db }
    }

    /// Opens database at [`database_location`].
    /// If database did not exist before, it is NOT initialized!
    pub fn open(database_location: impl AsRef<Path>) -> sled::Result<Self> {
        let db = sled::open(database_location)?;
        Ok(Self { db })
    }

    /// Open database at [`location_location`]`.
    /// If database did not exist, use data to initialize it.
    pub fn create_or_open(
        database_location: impl AsRef<Path>,
        data: ServerInfoData,
    ) -> Result<Self> {
        let server_db = Self::open(database_location)?;

        if server_db.is_init()? {
            return Ok(server_db);
        }
        info!("Creating new server data");
        server_db.set_server_data(data)?;

        Ok(server_db)
    }

    /// Open database with name [`name`].
    /// Automatically (magically) find the location where databases are stored and select the database with name from it.
    /// If the database does not exist, creates it and initializes it with new server data using current time as uuid seed.
    pub fn magic_open_server(name: String) -> Result<Self> {
        let mut path = ServerConfig::get().get_database_directory();
        path.push(name.as_str());
        let uuid = Uuid::now_v7();
        Self::create_or_open(path, ServerInfoData { name, uuid })
    }
    /// Open database with corresponding to [`uuid`].
    /// Automatically (magically) find the location where databases are stored and select the database with corresponding uuid from it.
    /// If the database does not exist, creates it and initializes it with new server data.
    pub fn magic_open_client(name: String, uuid: Uuid) -> Result<Self> {
        let mut path = ClientConfig::get().get_database_directory();
        path.push(uuid.to_string().as_str());
        Self::create_or_open(path, ServerInfoData { name, uuid })
    }

    /// Queries the database, if initialized (server data was set) return true.
    pub fn is_init(&self) -> Result<bool> {
        let tree = SERVER_DATA_TABLE.open(&self.db)?;
        Ok(tree.has_single()?)
        //Ok(tree.get_single()?.is_some())
    }

    /// Get server data.
    /// Run [`ServerDB::is_init()`] first to check if it's safe to get data
    pub fn get_server_data(&self) -> Result<ServerInfoData> {
        let tree = SERVER_DATA_TABLE.open(&self.db)?;
        tree.get_single()?.context("Expect data in server_data, run is_init() before accessing or set_server_data() on db.")
    }
    /// Set server data.
    /// Either replaces existing data with new one or initializes the database with corresponding data.
    pub fn set_server_data(&self, data: ServerInfoData) -> Result<()> {
        let tree = SERVER_DATA_TABLE.open(&self.db)?;
        tree.set_single(data)?;
        Ok(())
    }

    /// Get DBTree of all Messages, allowing querying, and storing data.
    pub fn messages(&self) -> OpenResult<MessagesTable> {
        MESSAGES_TABLE.open(&self.db)
    }
    /// Get DBTree of all Channels, allowing querying, and storing data.
    pub fn channels(&self) -> OpenResult<ChannelsTable> {
        CHANNELS_TABLE.open(&self.db)
    }
    /// Get DBTree of all Users, allowing querying, and storing data.
    pub fn users(&self) -> OpenResult<UsersTable> {
        USERS_TABLE.open(&self.db)
    }

    /// Only meant to be used for extending functionality of DB, you should not use this directly.
    pub fn get_raw(&self) -> &sled::Db {
        &self.db
    }
}

#[cfg(test)]
mod test {
    use std::array;

    use super::*;
    use anyhow::{Context, Result};

    // ======================================
    // => Temp Tests
    // ======================================
    #[test]
    fn test_read_messages() -> Result<()> {
        let server = DB::mock();
        let user_key = UserKey::new_now();
        let mock_messages: [_; 50] =
            array::from_fn(|i| MessageData::now(user_key, format!("some test message {i}")));

        let channel_key = ChannelKey::new_now();
        server
            .messages()?
            .insert_in_context(channel_key, mock_messages)?;

        let another_key = MessageKey::new_now(channel_key);
        server.messages()?.set(
            another_key,
            MessageData::now(user_key, format!("Another message")),
        )?;

        let another_key = MessageKey::new_now(channel_key);
        server.messages()?.set(
            another_key,
            MessageData::now(user_key, format!("Another message")),
        )?;

        let (key, _) = server
            .messages()?
            .first()?
            .context("No elements in messages")?;
        let messages = server.messages()?;
        for next in messages.range(key..).take(10) {
            let (_k, value) = next?;
            println!("Found message: <{}>: {}", value.timestamp, value.content);
        }
        Ok(())
    }

    #[test]
    fn test_insert_and_read() -> Result<()> {
        let server = DB::mock();

        let user1 = UserData::new("user_1".to_string());
        let user_key = server.users()?.insert(user1)?;
        println!("inserted user1");
        let channel_key = ChannelKey::new_now();
        let channel1 = ChannelData::text("Channel 1".to_string(), "An example channel".to_string());
        server.channels()?.set(channel_key, channel1)?;
        println!("inserted channel1");
        let message1 = MessageData::now(user_key, "Test Message 1".to_string());
        let message_key = MessageKey::new_now(channel_key);
        server.messages()?.set(message_key, message1)?;
        println!("inserted message1");

        println!("reading data");
        println!();

        let got_channel = server
            .channels()?
            .get(channel_key)?
            .expect("Expected channel that was just inserted");
        let got_message1 = server
            .messages()?
            .get(message_key)?
            .expect("Expected message value that was just inserted");
        let message_user_key = got_message1.author;
        let got_user = server
            .users()?
            .get(message_user_key)?
            .expect("Expected user value that was just inserted");
        println!(
            "Had message: @\"{}\" <{}> {}: {}",
            got_channel.get_name(),
            got_message1.timestamp,
            got_user.name,
            got_message1.content
        );
        Ok(())
    }
}
