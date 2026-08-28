pub mod client_session;

use crate::{
    common::lens::Lens,
    schema::{DataStore, SideData, SidedData, client::client_session::ClientSession},
};
use speakrs_storage::{
    table::SerdeSingleton,
    tree::{TreeResult, TypedTree},
};

pub type ClientDataStore = DataStore<ClientOnlyData>;
pub(self) type ClLens<'db, T> = Lens<'db, T, ClientOnlyData>;
pub(self) type ClLensResult<'db, T> = TreeResult<ClLens<'db, T>>;

#[derive(Debug, Clone)]
pub struct ClientOnlyData {
    current_session: SerdeSingleton<ClientSession>,
}

impl SideData for ClientOnlyData {
    fn as_enum(&self) -> SidedData<'_> {
        SidedData::Client(self)
    }

    fn new(db: &sled::Db) -> sled::Result<Self> {
        Ok(ClientOnlyData {
            current_session: TypedTree::open(db, "current_session")?,
        })
    }
}
