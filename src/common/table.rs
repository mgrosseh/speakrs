use crate::common::{
    codec::{PodCodec, SerdeJsonCodec},
    key::{SingletonKey, UuidKey, compound::CompoundKey},
    tree::TypedTree,
};
use std::marker::PhantomData;

#[derive(Debug, thiserror::Error)]
#[error("Failed opening database table '{tree_name}': {db_error}")]
pub struct TableOpenError {
    pub tree_name: &'static str,
    pub db_error: sled::Error,
}

pub type OpenResult<T> = Result<T, TableOpenError>;

impl<V, K, Codec> TableDecl<TypedTree<V, K, Codec>> {
    /// Open the tree in [`db`], if it doesn't exist yet, potentially creates it.
    pub fn open(&self, db: &sled::Db) -> OpenResult<TypedTree<V, K, Codec>> {
        db.open_tree(self.0)
            .map(TypedTree::from_raw)
            .map_err(|db_error| TableOpenError {
                tree_name: self.0,
                db_error,
            })
    }
}

pub struct TableDecl<Tree>(&'static str, PhantomData<Tree>);

impl<V, K, Codec> TypedTree<V, K, Codec> {
    pub const fn decl(tree_name: &'static str) -> TableDecl<Self> {
        TableDecl(tree_name, PhantomData)
    }
}

pub type OneToMany<OneKey, ManyKey, RelationshipData = (), Codec = PodCodec> =
    TypedTree<CompoundKey<(OneKey, ManyKey)>, RelationshipData, Codec>;

pub type SerdeTree<T, K = UuidKey<T>> = TypedTree<K, T, SerdeJsonCodec>;
pub type SerdeSingleton<T> = TypedTree<SingletonKey, T, SerdeJsonCodec>;

/// An abstract database table of specific row value, primary and foregin keys.
struct Table<Row, PrimaryKey, Relationships, Codec, KeyGen> {
    primary_tree: TypedTree<PrimaryKey, Row, Codec>,
    relationship_trees: Relationships,
    _marker: PhantomData<(Codec, KeyGen)>,
}
