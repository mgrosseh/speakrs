use crate::common::{
    codec::{PodCodec, SerdeJsonCodec},
    key::{PrefixedKey, SingletonKey, UuidKey, UuidNowKeygen, compound::CompoundKey},
    tree::DBTree,
};
use std::marker::PhantomData;

#[derive(Debug, thiserror::Error)]
#[error("Failed opening database table '{tree_name}': {db_error}")]
pub struct TableOpenError {
    pub tree_name: &'static str,
    pub db_error: sled::Error,
}

pub type OpenResult<T> = Result<T, TableOpenError>;

impl<V, K, Codec, Gen> TableDecl<DBTree<V, K, Codec, Gen>> {
    /// Open the tree in [`db`], if it doesn't exist yet, potentially creates it.
    pub fn open(&self, db: &sled::Db) -> OpenResult<DBTree<V, K, Codec, Gen>> {
        db.open_tree(self.0)
            .map(DBTree::from_raw)
            .map_err(|db_error| TableOpenError {
                tree_name: self.0,
                db_error,
            })
    }
}

pub struct TableDecl<Tree>(&'static str, PhantomData<Tree>);

impl<V, K, Codec, Gen> DBTree<V, K, Codec, Gen> {
    pub const fn decl(tree_name: &'static str) -> TableDecl<Self> {
        TableDecl(tree_name, PhantomData)
    }
}

pub type OneToMany<OneKey, ManyKey, RelationshipData = (), Codec = PodCodec> =
    DBTree<CompoundKey<(OneKey, ManyKey)>, RelationshipData, Codec>;

pub type SerdeTree<T, K = UuidKey<T>, Gen = UuidNowKeygen> = DBTree<K, T, SerdeJsonCodec, Gen>;
pub type SerdeSingleton<T> = DBTree<SingletonKey, T, SerdeJsonCodec>;
