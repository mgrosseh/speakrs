use crate::common::{
    codec::SerdeJsonCodec,
    key::{SingletonKey, UuidKey, UuidNowKeygen},
    tree::DBTree,
};
use std::marker::PhantomData;

impl<V, K, Codec, Gen> TableDecl<DBTree<V, K, Codec, Gen>> {
    /// Open the tree in [`db`], if it doesn't exist yet, potentially creates it.
    pub fn open(&self, db: &sled::Db) -> sled::Result<DBTree<V, K, Codec, Gen>> {
        db.open_tree(self.0).map(DBTree::from_raw)
    }
}

pub struct TableDecl<Tree>(&'static str, PhantomData<Tree>);

impl<V, K, Codec, Gen> DBTree<V, K, Codec, Gen> {
    pub const fn decl(tree_name: &'static str) -> TableDecl<Self> {
        TableDecl(tree_name, PhantomData)
    }
}

pub type SerdeTree<T, K = UuidKey<T>, Gen = UuidNowKeygen> = DBTree<K, T, SerdeJsonCodec, Gen>;
pub type SerdeSingleton<T> = DBTree<SingletonKey, T, SerdeJsonCodec>;
