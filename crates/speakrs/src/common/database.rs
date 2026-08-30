use std::path::Path;

use eyre::Result;
use tracing::info;
use uuid::Uuid;

use crate::{
    client::ClientConfig,
    schema::{ClientDataStore, DataStore, ServerDataStore, SideData, server_info::ServerInfo},
    server::ServerConfig,
};

pub fn open_server_db(name: String) -> Result<ServerDataStore> {
    create_or_open(
        ServerConfig::get().get_database_directory(),
        &name.clone(),
        || ServerInfo {
            uuid: Uuid::now_v7(),
            name,
        },
    )
}

pub fn open_client_db(info: ServerInfo) -> Result<ClientDataStore> {
    create_or_open(
        ClientConfig::get().get_database_directory(),
        &info.uuid.to_string(),
        move || info,
    )
}

pub fn create_or_open<Side: SideData>(
    database_directory: impl AsRef<Path>,
    db_name: &str,
    get_info: impl FnOnce() -> ServerInfo,
) -> Result<DataStore<Side>> {
    let location = database_directory.as_ref().join(db_name);
    let store = DataStore::open(location)?;

    if store.is_init()? {
        return Ok(store);
    }
    info!("Creating new server info");
    store.set_server_info(get_info())?;
    Ok(store)
}

#[cfg(test)]
mod test {
    use eyre::ResultExt as _;
    use speakrs_storage::pagination::Pagination;

    use crate::schema::{channel::Channel, message::Message, user::User};

    use super::*;

    // ======================================
    // => Temp Tests
    // ======================================
    #[test]
    fn test_read_messages() -> Result<()> {
        let store = ServerDataStore::mock();
        let user1 = store.add_user(User::new("test user 1".to_string()))?;
        let user2 = store.add_user(User::new("test user 2".to_string()))?;

        let channel = store.add_channel(Channel::text(
            "Channel 1".to_string(),
            "An example channel".to_string(),
        ))?;

        for i in 0..15 {
            let user = if i % 3 == 0 { user1 } else { user2 };
            store.add_message(Message::now(
                user,
                channel,
                format!("Some test message {i}"),
            ))?;
        }

        let page = store.messages(Pagination::first(5))?;

        assert_eq!(
            page.nodes()
                .map(|msg| msg.content.as_str())
                .collect::<Vec<_>>(),
            vec![
                "Some test message 0",
                "Some test message 1",
                "Some test message 2",
                "Some test message 3",
                "Some test message 4",
            ]
        );
        assert_eq!(page.has_next_page, true);

        let page = store.user(user1)?.messages(Pagination::first(8))?;
        assert_eq!(
            page.nodes()
                .map(|msg| msg.content.as_str())
                .collect::<Vec<_>>(),
            vec![
                "Some test message 0",
                "Some test message 3",
                "Some test message 6",
                "Some test message 9",
                "Some test message 12",
            ]
        );
        assert_eq!(page.has_next_page, false);

        let page = store.user(user2)?.messages(Pagination::first(3))?;
        assert_eq!(
            page.nodes()
                .map(|msg| msg.content.as_str())
                .collect::<Vec<_>>(),
            vec![
                "Some test message 1",
                "Some test message 2",
                "Some test message 4",
            ]
        );
        assert_eq!(page.has_next_page, true);


        Ok(())
    }

    #[test]
    fn test_insert_and_read() -> Result<()> {
        let db = ServerDataStore::mock();

        let user1 = User::new("user_1".to_string());
        let user_key = db.add_user(user1)?;
        println!("inserted user1");
        let channel1 = Channel::text("Channel 1".to_string(), "An example channel".to_string());
        let channel_key = db.add_channel(channel1)?;
        println!("inserted channel1");
        let message1 = Message::now(user_key, channel_key, "Test Message 1".to_string());
        let message_key = db.add_message(message1)?;
        println!("inserted message1");

        println!("reading data");
        println!();

        let got_channel = db
            .channel(channel_key)
            .wrap_err("Expected channel that was just inserted")?;
        let got_message1 = db
            .message(message_key)
            .wrap_err("Expected message value that was just inserted")?;
        let got_user = got_message1.author()?;
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
