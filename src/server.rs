use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr}, path::PathBuf
};

use auth::{Permissions, authenticate_session, permission_guard, register_user};
use tarpc::{
    context::Context,
    server::{Channel, incoming::Incoming},
    tokio_serde::formats::Json,
};

use crate::common::{
    self, audio::AudioPacket, auth::SessionToken, database::DB, rpc::{RpcService, ServiceResult}, schema::{ChannelData, ChannelKey, MessageData, MessageKey, ServerInfoData, UserData, UserKey}
};

use futures::{future, prelude::*};

mod auth;
pub use auth::AuthError;

mod server_schema;

// TODO: use pagination (see `cursor`) instead of new_X_since

#[derive(Debug, clap::Parser)]
pub(crate) struct ServerArguments {
    /// Port to serve tcp commands under (default: 51777)
    #[clap(short, long, default_value_t = 51777)]
    port: u16,
    // TODO: wording may be bad: ;; also write a manual
    /// name of the server, this will determine where to store server database, it should be unique in any given system.
    /// Clients will use the internal uuid of the server not the name, so a client can connect with multiple servers of the same name.
    /// On the same system only one server can exist with this name.
    ///
    /// If no name is provided, uses "default_server".
    ///
    /// If unsure consult the manual.
    #[clap(short, long, default_value_t="default_server".to_string())]
    name: String,
    /// Be verbose
    /// Also consider: RUST_LOG=debug to be even more verbose
    #[clap(short, long, default_value_t = false)]
    verbose: bool,
    /// Use ipv6 instead of ipv4
    #[clap(short, long, default_value_t = false)]
    ipv6: bool,
}

pub(crate) async fn run(args: ServerArguments) -> anyhow::Result<()> {
    let db = DB::magic_open_server(args.name.to_string())?;
    command_server(args, db).await?;
    Ok(())
}

// ======================================
// => Server Config
// ======================================
// NOTE: For Devs: Try to annotate every value with `///` and explain what it does
#[derive(Clone, serde::Deserialize, serde::Serialize)]
pub struct ServerConfig {
    /// Database related settings
    database: ServerConfigDatabase,
}
#[derive(Clone, serde::Deserialize, serde::Serialize)]
pub struct ServerConfigDatabase {
    /// Directory to store server databases in, if empty stores databases next to config.
    /// If set to `/some/dir` creates `/some/dir/server` and `/some/dir/server/<server_name>` for each database
    directory: Option<String>,
}
impl ServerConfig {
    /// See [`ServerConfig::database`]
    pub fn get_database_directory(&self) -> PathBuf {
        if self.database.directory.is_some() {
            return PathBuf::from(self.database.directory.clone().unwrap());
        }
        let mut path = common::config_home();
        path.push("databases");
        path.push("server");
        path
    }
    /// Get ServerConfig from cached unified Config.
    /// This is a relative expensive operation (clones ServerConfig from R/W locked Config value), it might be deprecated in the future.
    /// TODO: currently throws an error if config does not have a server section
    pub fn get() -> Self {
        common::Config::clone_server()
            .expect("Running server requires config to have server section")
    }
}

// ======================================
// => RPC
// ======================================
// This is the type that implements the generated World trait. It is the business logic
// and is used to start the server.
#[derive(Clone)]
struct HelloServer {
    #[allow(unused)] // TODO: consider if needed
    addr: SocketAddr,
    db: DB,
}

impl HelloServer {
    pub fn new(addr: SocketAddr, db: DB) -> Self {
        Self { addr, db }
    }
}

impl common::rpc::RpcService for HelloServer {

    async fn get_server_data(self, _: Context) -> ServiceResult<ServerInfoData> {
        Ok(self.db.get_server_data()?)
    }

    async fn register_user(self, _: Context, data: UserData, password: String) -> ServiceResult<UserKey> {
        register_user(self.db, data, password)
    }
    async fn authenticate_session(self, _: Context, user: UserKey, password: String) -> ServiceResult<SessionToken> {
        authenticate_session(self.db, user, password)
    }

    async fn get_new_messages_since(
        self,
        _: Context,
        _session: SessionToken,
        since: Option<MessageKey>,
    ) -> ServiceResult<Vec<(MessageKey, MessageData)>> {
        // TODO: permissions, pagination
        Ok(if since.is_none() {
            self.db
                .messages()?
                .range(..)
                .collect::<anyhow::Result<Vec<_>>>()?
        } else {
            self.db
                .messages()?
                .range(since.unwrap()..)
                .skip(1)
                .collect::<anyhow::Result<Vec<_>>>()?
        })
    }
    async fn insert_message(
        self,
        _: Context,
        session: SessionToken,
        channel: ChannelKey,
        data: MessageData,
    ) -> ServiceResult<MessageKey> {
        permission_guard(self.db.clone(), session, &[Permissions::CanWriteMessageIn(channel)])?;
        self.db
            .messages()?
            .insert_in_context(channel, data)
            .map_err(|e| e.into())
    }
    async fn get_message(self, _: Context, session: SessionToken, key: MessageKey) -> ServiceResult<Option<MessageData>> {
        permission_guard(self.db.clone(), session, &[Permissions::CanReadMessageIn(key.prefix())])?;
        self.db.messages()?.get(key).map_err(|e| e.into())
    }

    async fn get_user_info(self, _: Context, session: SessionToken, key: UserKey) -> ServiceResult<Option<UserData>> {
        permission_guard(self.db.clone(), session, &[Permissions::CanSeeUser(key)])?;
        self.db.users()?.get(key).map_err(|e| e.into())
    }

    async fn create_channel(
        self,
        _: Context,
        session: SessionToken,
        data: ChannelData,
    ) -> ServiceResult<ChannelKey> {
        permission_guard(self.db.clone(), session, &[Permissions::CanCreateChannel])?;
        self.db.channels()?.insert(data).map_err(|e| e.into())
    }
    async fn get_channel(self, _: Context, _session: SessionToken, key: ChannelKey) -> ServiceResult<Option<ChannelData>> {
        self.db.channels()?.get(key).map_err(|e| e.into())
    }
    async fn get_new_channels_since(
        self,
        _: Context,
        _session: SessionToken,
        since: Option<ChannelKey>,
    ) -> ServiceResult<Vec<(ChannelKey, ChannelData)>> {
        // TODO: permissions, pagination
        // TODO: ideally there would be a per-channel basis on whether to allow receiving it
        Ok(if since.is_none() {
            self.db
                .channels()?
                .range(..)
                .collect::<anyhow::Result<Vec<_>>>()?
        } else {
            self.db
                .channels()?
                .range(since.unwrap()..)
                .skip(1)
                .collect::<anyhow::Result<Vec<_>>>()?
        })
    }

    async fn send_audio(self, _: Context, _session: SessionToken, _packet: AudioPacket) -> ServiceResult<()> {
        panic!("todo"); // TODO
    }
}

#[tracing::instrument(skip(server))]
async fn command_server(args: ServerArguments, server: DB) -> anyhow::Result<()> {
    let server_addr = if args.ipv6 {
        (IpAddr::V6(Ipv6Addr::LOCALHOST), args.port)
    } else {
        (IpAddr::V4(Ipv4Addr::LOCALHOST), args.port)
    };
    if args.verbose {
        println!(
            "Serving under addr {} on port {}",
            server_addr.0, server_addr.1
        );
    }
    let mut listener = tarpc::serde_transport::tcp::listen(&server_addr, Json::default).await?;
    tracing::info!("Listening on port {}", listener.local_addr().port());
    listener.config_mut().max_frame_length(usize::MAX);
    listener
        // Ignore accept errors.
        .filter_map(|r| future::ready(r.ok()))
        .map(tarpc::server::BaseChannel::with_defaults)
        // Limit channels to 1 per IP.
        .max_channels_per_key(1, |t| t.transport().peer_addr().unwrap().ip())
        // serve is generated by the service attribute. It takes as input any type implementing
        // the generated World trait.
        .map(|channel| {
            let peer_addr = channel.transport().peer_addr().unwrap();
            let server = HelloServer::new(peer_addr, server.clone());
            channel.execute(server.serve()).for_each(|fut| async {
                tokio::spawn(fut);
            })
        })
        // Max 10 channels.
        .buffer_unordered(10)
        .for_each(|_| async {})
        .await;

    Ok(())
}
