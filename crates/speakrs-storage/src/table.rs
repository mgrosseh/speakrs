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

// /// An abstract database table of specific row value, primary and foregin keys.
// struct Table<K, V, Rels, Codec, KeyGen> {
//     primary_tree: TypedTree<K, EncodedValue<V, Codec>>,
//     relationships: Rels,
//     _marker: PhantomData<KeyGen>,
// }

// trait TableAccess<K> {
//     type Encoded;

//     /// Returns the number of rows in this table.
//     ///
//     /// Beware: performs a full O(n) scan under the hood.
//     #[allow(unused)]
//     fn len(&self) -> usize;

//     /// Returns `true` if the `Table` contains no elements.
//     #[allow(unused)]
//     fn is_empty(&self) -> bool;

//     /// Get a value corresponding to the key, or [`None`] if no value is present.
//     fn get(&self, key: impl Borrow<K>) -> sled::Result<Option<Self::Encoded>>;

//     /// Inserts an already encoded key-value pair into the tree.
//     ///
//     /// If the tree did not have this key present, [`None`] is returned.
//     ///
//     /// If the tree did have this key present, the value is updated, and the old value is returned.
//     fn add(&self, value: Self::Encoded) -> sled::Result<Option<Self::Encoded>>;
//     fn delete(&self, key: impl Borrow<K>) -> sled::Result<Option<Self::Encoded>>;
// }
// impl<K, V, Rels, Codec, KeyGen> Table<K, V, Rels, Codec, KeyGen> {}

// trait DbRow {
//     type Key: DbKey;
//     type Data;
//     type Codec: DbValueCodec<Self::Data>;
// }
