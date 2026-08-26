use crate::{
    codec::{EncodedValue, SerdeJsonCodec},
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

impl<K, Encoded> TableDecl<TypedTree<K, Encoded>> {
    /// Open the tree in [`db`], if it doesn't exist yet, potentially creates it.
    pub fn open(&self, db: &sled::Db) -> OpenResult<TypedTree<K, Encoded>> {
        db.open_tree(self.0)
            .map(TypedTree::from_raw)
            .map_err(|db_error| TableOpenError {
                tree_name: self.0,
                db_error,
            })
    }
}

pub struct TableDecl<Tree>(&'static str, PhantomData<Tree>);

impl<V, Encoded> TypedTree<V, Encoded> {
    pub const fn decl(tree_name: &'static str) -> TableDecl<Self> {
        TableDecl(tree_name, PhantomData)
    }
}

pub type OneToMany<OneKey, ManyKey, RelationshipData = ()> =
    TypedTree<CompoundKey<(OneKey, ManyKey)>, RelationshipData>;

pub type SerdeTree<T, K = UuidKey<T>> = TypedTree<K, EncodedValue<T, SerdeJsonCodec>>;
pub type SerdeSingleton<T> = TypedTree<SingletonKey, EncodedValue<T, SerdeJsonCodec>>;
