use crate::common::{codec::SerdeJsonCodec, key::UuidKey, tree::DBTree};
use std::marker::PhantomData;

pub struct TableDecl<Tree>(&'static str, PhantomData<Tree>);

impl<Tree> TableDecl<Tree> {
    pub const fn named(tree_name: &'static str) -> Self {
        Self(tree_name, PhantomData)
    }
}

impl<V, K, Codec> TableDecl<DBTree<V, K, Codec>> {
    /// Open the tree in [`db`], if it doesn't exist yet, potentially creates it.
    pub fn open(&self, db: &sled::Db) -> sled::Result<DBTree<V, K, Codec>> {
        db.open_tree(self.0).map(DBTree::from_raw)
    }
}

pub type SerdeTree<T, K = UuidKey<T>> = DBTree<K, T, SerdeJsonCodec>;
