use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    path::PathBuf,
};

use auth::{Permissions, authenticate_session, permission_guard};
use speakrs_storage::pagination::{Page, Pagination};
use tarpc::{
    context::Context,
    server::{Channel as _, incoming::Incoming as _},
    tokio_serde::formats::Json,
};
use tracing::info;

use crate::{
    common::{
        self,
        audio::AudioPacket,
        config::{Config, config_home},
        database::open_server_db,
        rpc::{RpcService, ServiceResult},
    },
    schema::{
        ServerDataStore,
        channel::{Channel, ChannelId},
        message::{Message, MessageId},
        server::session::SessionToken,
        server_info::ServerInfo,
        user::{User, UserId},
    },
};

use futures::{future, prelude::*};

mod auth;
pub use auth::AuthError;

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
    /// Use ipv6 instead of ipv4
    #[clap(short, long, default_value_t = false)]
    ipv6: bool,
    /// Dump the database to file
    #[clap(long)]
    dump_db_to: Option<String>,
}

pub(crate) async fn run(args: ServerArguments) -> eyre::Result<()> {
    let db = open_server_db(args.name.to_string())?;
    if let Some(_path) = args.dump_db_to {
        // TODO
        // let dump = db.dump()?;
        // let file = File::create(path)?;
        // serde_json::to_writer_pretty(file, &dump)?;
        return Ok(());
    }
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
        let mut path = config_home();
        path.push("databases");
        path.push("server");
        path
    }
    /// Get ServerConfig from cached unified Config.
    /// This is a relative expensive operation (clones ServerConfig from R/W locked Config value), it might be deprecated in the future.
    /// TODO: currently throws an error if config does not have a server section
    pub fn get() -> Self {
        Config::clone_server().expect("Running server requires config to have server section")
    }
}

// ======================================
// => RPC
// ======================================
#[derive(Debug, Clone)]
struct RpcServer {
    #[allow(unused)] // TODO: consider if needed
    addr: SocketAddr,
    db: ServerDataStore,
}

impl RpcServer {
    pub fn new(addr: SocketAddr, db: ServerDataStore) -> Self {
        Self { addr, db }
    }
}

impl common::rpc::RpcService for RpcServer {
    #[tracing::instrument]
    async fn get_server_data(self, _: Context) -> ServiceResult<ServerInfo> {
        Ok(self.db.server_info()?)
    }

    #[tracing::instrument]
    async fn register_user(
        self,
        _: Context,
        name: String,
        password: String,
    ) -> ServiceResult<UserId> {
        Ok(self.db.register_user(name, &password)?)
    }
    #[tracing::instrument]
    async fn authenticate_session(
        self,
        _: Context,
        user: UserId,
        password: String,
    ) -> ServiceResult<SessionToken> {
        authenticate_session(&self.db, user, password)
    }

    async fn validate_session(self, _: Context, token: SessionToken) -> ServiceResult<bool> {
        auth::validate_session(&self.db, token)
    }

    async fn get_channel_messages(
        self,
        _: Context,
        _session: SessionToken,
        since: Option<MessageId>,
    ) -> ServiceResult<Page<Message, MessageId>> {
        // TODO: permissions
        Ok(self
            .db
            .messages(Pagination::last(100).opt_after(since))?
            .focus)
    }
    async fn insert_message(
        self,
        _: Context,
        session: SessionToken,
        channel: ChannelId,
        content: String,
    ) -> ServiceResult<MessageId> {
        permission_guard(
            &self.db,
            session,
            &[Permissions::CanWriteMessageIn(channel)],
        )?;
        let user = self.db.session(session)?.user;
        Ok(self.db.add_message(Message::now(user, channel, content))?)
    }
    async fn get_message(
        self,
        _: Context,
        session: SessionToken,
        key: MessageId,
    ) -> ServiceResult<Message> {
        let message = self.db.message(key)?;
        permission_guard(
            &self.db,
            session,
            &[Permissions::CanReadMessageIn(message.channel)],
        )?;
        Ok(message.focus.node)
    }

    async fn get_user_info(
        self,
        _: Context,
        session: SessionToken,
        key: UserId,
    ) -> ServiceResult<User> {
        permission_guard(&self.db, session, &[Permissions::CanSeeUser(key)])?;
        Ok(self.db.user(key)?.focus.node)
    }

    async fn create_channel(
        self,
        _: Context,
        session: SessionToken,
        data: Channel,
    ) -> ServiceResult<ChannelId> {
        permission_guard(&self.db, session, &[Permissions::CanCreateChannel])?;
        Ok(self.db.add_channel(data)?)
    }
    async fn get_channel(
        self,
        _: Context,
        _session: SessionToken,
        key: ChannelId,
    ) -> ServiceResult<Channel> {
        Ok(self.db.channel(key)?.focus.node)
    }
    async fn get_channels(
        self,
        _: Context,
        _session: SessionToken,
        pagination: Pagination<ChannelId>,
    ) -> ServiceResult<Page<Channel, ChannelId>> {
        // TODO: permissions
        // TODO: ideally there would be a per-channel basis on whether to allow receiving it
        Ok(self.db.channels(pagination)?.focus)
    }

    async fn send_audio(
        self,
        _: Context,
        _session: SessionToken,
        _packet: AudioPacket,
    ) -> ServiceResult<()> {
        panic!("todo"); // TODO
    }
}

#[tracing::instrument(skip(db))]
async fn command_server(args: ServerArguments, db: ServerDataStore) -> eyre::Result<()> {
    let server_addr = if args.ipv6 {
        (IpAddr::V6(Ipv6Addr::UNSPECIFIED), args.port)
    } else {
        (IpAddr::V4(Ipv4Addr::UNSPECIFIED), args.port)
    };
    info!(
        "Serving under addr {} on port {}",
        server_addr.0, server_addr.1
    );
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
        .map(move |channel| {
            let peer_addr = channel.transport().peer_addr().unwrap();
            let server = RpcServer::new(peer_addr, db.clone());
            info!("Peer connected from {peer_addr}");
            channel.execute(server.serve()).for_each(move |fut| async {
                tokio::spawn(fut);
            })
        })
        // Max 10 channels.
        .buffer_unordered(10)
        .for_each(|_| async {})
        .await;

    Ok(())
}
