// ======================================
// => specific value / key implementation
// ======================================

use std::{fmt::Debug, path::Path};

use sled::transaction::ConflictableTransactionError;

use speakrs_storage::{
    codec::SerdeJsonCodec,
    table::{OneToMany, Primary, SerdeSingleton},
    tree::{TreeError, TreeResult, TypedTree},
};

use crate::{
    common::lens::Lens,
    schema::{
        channel::{Channel, ChannelId},
        client::ClientOnlyData,
        message::{Message, MessageId},
        server::ServerOnlyData,
        server_info::ServerInfo,
        user::{User, UserId},
    },
};
pub mod channel;
pub mod client;
pub mod message;
pub mod server;
pub mod server_info;
pub mod user;

pub use client::ClientDataStore;
pub use server::ServerDataStore;
pub use server::session::SessionToken;

type LensResult<'db, T, Side> = TreeResult<Lens<'db, T, Side>>;

#[derive(Debug, Clone)]
pub struct DataStore<Side> {
    users: Primary<User, SerdeJsonCodec>,
    channels: Primary<Channel, SerdeJsonCodec>,
    messages: Primary<Message, SerdeJsonCodec>,
    messages_by_channel: OneToMany<ChannelId, MessageId>,
    messages_by_author: OneToMany<UserId, MessageId>,
    server_data: SerdeSingleton<ServerInfo>,
    pub(self) side: Side,
}

pub trait SideData {
    fn new(db: &sled::Db) -> sled::Result<Self>
    where
        Self: Sized;

    #[allow(unused)] // TODO
    fn as_enum(&self) -> SidedData<'_>;
}

#[allow(unused)] // TODO
pub enum SidedData<'a> {
    Client(&'a ClientOnlyData),
    Server(&'a ServerOnlyData),
}

#[derive(thiserror::Error, Debug)]
#[error("Key not found: {0}")]
struct IdNotFound<T>(T);

impl<T> From<IdNotFound<T>> for TreeError
where
    IdNotFound<T>: Into<anyhow::Error>,
{
    fn from(error: IdNotFound<T>) -> Self {
        TreeError::Other(error.into())
    }
}

impl<T> From<IdNotFound<T>> for ConflictableTransactionError<TreeError>
where
    IdNotFound<T>: Into<anyhow::Error>,
{
    fn from(value: IdNotFound<T>) -> Self {
        ConflictableTransactionError::Abort(TreeError::from(value))
    }
}

impl<S> DataStore<S> {
    pub fn open(database_location: impl AsRef<Path>) -> sled::Result<Self>
    where
        S: SideData,
    {
        Self::with_db(sled::open(database_location)?)
    }

    // region: Constructors
    fn with_db(db: sled::Db) -> sled::Result<Self>
    where
        S: SideData,
    {
        Ok(Self {
            users: TypedTree::open(&db, "users")?,
            channels: TypedTree::open(&db, "channels")?,
            messages: TypedTree::open(&db, "messages")?,
            messages_by_channel: TypedTree::open(&db, "messages_by_channel")?,
            messages_by_author: TypedTree::open(&db, "messages_by_author")?,
            server_data: TypedTree::open(&db, "server_data")?,
            side: S::new(&db)?,
        })
    }

    #[cfg(test)]
    pub fn mock() -> Self
    where
        S: SideData,
    {
        let db = sled::Config::new().temporary(true).open().expect("db open");
        Self::with_db(db).expect("DataStore open")
    }

    fn lens<T>(&self, focus: T) -> Lens<'_, T, S> {
        Lens { store: self, focus }
    }
}
