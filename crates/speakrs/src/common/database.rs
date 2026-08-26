use std::{collections::HashMap, path::Path};

use anyhow::Result;
use speakrs_storage::{codec::Encodable, table::OpenResult};
use tracing::info;
use uuid::Uuid;

use crate::{client::ClientConfig, common::schema::*, server::ServerConfig};

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
        Ok(tree.get_single()?.is_some())
    }

    /// Get server data.
    /// Run [`ServerDB::is_init()`] first to check if it's safe to get data
    pub fn get_server_data(&self) -> Result<ServerInfoData> {
        let tree = SERVER_DATA_TABLE.open(&self.db)?;
        let Some(encoded) = tree.get_single()? else {
            anyhow::bail!(
                "Expect data in server_data, run is_init() before accessing or set_server_data() on db."
            );
        };
        Ok(encoded.decode()?)
    }
    /// Set server data.
    /// Either replaces existing data with new one or initializes the database with corresponding data.
    pub fn set_server_data(&self, data: ServerInfoData) -> Result<()> {
        let tree = SERVER_DATA_TABLE.open(&self.db)?;
        tree.insert_single(data.encode()?)?;
        Ok(())
    }

    /// Get DBTree of all Messages, allowing querying, and storing data.
    pub fn messages(&self) -> OpenResult<MessagesTable> {
        MESSAGES_TABLE.open(&self.db)
    }

    pub fn messages_in_channel(&self) -> OpenResult<MessagesInChannelTable> {
        MESSAGES_IN_CHANNEL_TABLE.open(&self.db)
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

    /// Get a dump of all data in this database.
    /// WARNING: this will read and the attempt to store in-memory the whole database.
    /// If it is too big it may not fit into memory.
    pub fn dump_shared(&self) -> Result<DBCommonDump> {
        let server_info = self.get_server_data()?;
        let mut messages = HashMap::new();
        for result in self.messages()?.iter() {
            let (key, value) = result?;
            messages.insert(key, value.decode()?);
        }
        let mut channels = HashMap::new();
        for result in self.channels()?.iter() {
            let (key, value) = result?;
            channels.insert(key, value.decode()?);
        }
        let mut users = HashMap::new();
        for result in self.users()?.iter() {
            let (key, value) = result?;
            users.insert(key, value.decode()?);
        }

        Ok(DBCommonDump {
            server_info,
            messages,
            channels,
            users,
        })
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct DBCommonDump {
    server_info: ServerInfoData,
    messages: HashMap<MessageKey, MessageData>,
    channels: HashMap<ChannelKey, ChannelData>,
    users: HashMap<UserKey, UserData>,
}

#[cfg(test)]
mod test {
    use anyhow::Context;
    use sled::transaction::ConflictableTransactionError;
    use speakrs_storage::{key::compound::ConsKey, tree::Tx};

    use super::*;

    // ======================================
    // => Temp Tests
    // ======================================
    #[test]
    fn test_read_messages() -> Result<()> {
        let server = DB::mock();
        let user_key = UserKey::new_now();
        let channel_key = ChannelKey::new_now();
        let mock_messages: [_; 50] = std::array::from_fn(|i| {
            MessageData::now(user_key, channel_key, format!("some test message {i}"))
        });
        let messages = server.messages()?;
        let messages_in_channel = server.messages_in_channel()?;

        Tx((&messages, &messages_in_channel)).transaction(|(messages, relation)| {
            for msg_data in &mock_messages {
                let key = MessageKey::new_now();
                let channel_key = msg_data.channel;
                let encoded = msg_data
                    .encode()
                    .map_err(ConflictableTransactionError::Abort)?;
                messages.insert(key, encoded)?;
                relation.insert(ConsKey::new((channel_key, key)), ())?;
            }

            let another_key = MessageKey::new_now();
            messages.insert(
                another_key,
                MessageData::now(user_key, channel_key, format!("Another message"))
                    .encode()
                    .map_err(ConflictableTransactionError::Abort)?,
            )?;
            relation.insert(ConsKey::new((channel_key, another_key)), ())?;

            let another_key = MessageKey::new_now();
            messages.insert(
                another_key,
                MessageData::now(user_key, channel_key, format!("Another message"))
                    .encode()
                    .map_err(ConflictableTransactionError::Abort)?,
            )?;
            relation.insert(ConsKey::new((channel_key, another_key)), ())?;

            Ok(())
        })?;

        let (key, _) = server
            .messages()?
            .first()?
            .context("No elements in messages")?;
        let messages = server.messages()?;
        for next in messages.range(key..).decode().take(10) {
            let (_k, value) = next?;
            println!(
                "Found message (channel {}): <{}>: {}",
                value.channel, value.timestamp, value.content
            );
        }
        Ok(())
    }

    #[test]
    fn test_insert_and_read() -> Result<()> {
        let server = DB::mock();

        let user1 = UserData::new("user_1".to_string());
        let user_key = UserKey::new_now();
        server.users()?.insert(user_key, user1.encode()?)?;
        println!("inserted user1");
        let channel_key = ChannelKey::new_now();
        let channel1 = ChannelData::text("Channel 1".to_string(), "An example channel".to_string());
        server.channels()?.insert(channel_key, channel1.encode()?)?;
        println!("inserted channel1");
        let message1 = MessageData::now(user_key, channel_key, "Test Message 1".to_string());
        let message_key = MessageKey::new_now();
        server.messages()?.insert(message_key, message1.encode()?)?;
        println!("inserted message1");

        println!("reading data");
        println!();

        let got_channel = server
            .channels()?
            .get(channel_key)?
            .context("Expected channel that was just inserted")?
            .decode()?;
        let got_message1 = server
            .messages()?
            .get(message_key)?
            .context("Expected message value that was just inserted")?
            .decode()?;
        let message_user_key = got_message1.author;
        let got_user = server
            .users()?
            .get(message_user_key)?
            .context("Expected user value that was just inserted")?
            .decode()?;
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
