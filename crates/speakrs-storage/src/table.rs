use sled::transaction::{ConflictableTransactionResult, UnabortableTransactionError, abort};

use crate::{
    codec::{DbValueCodec, DecodeExt, Encodable, EncodedValue, SerdeJsonCodec},
    key::{
        DbKey, SingletonKey, UuidKey,
        compound::{CompoundKey, ConsKey, KCons, KNil},
    },
    pagination::Edge,
    tree::{TreeError, TreeResult, TypedTransactionalTree, TypedTree},
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

pub type OneToOne<OneKey, T, Codec> = TypedTree<OneKey, EncodedValue<T, Codec>>;

impl<OneKey, ManyKey> TypedTransactionalTree<ConsKey<KCons<OneKey, KCons<ManyKey, KNil>>>, ()>
where
    CompoundKey<(OneKey, ManyKey)>: DbKey,
{
    /// Returns `true` if relation was inserted, i.e. it didn't already exist.
    pub fn insert_relation(
        &self,
        one: OneKey,
        many: ManyKey,
    ) -> Result<bool, UnabortableTransactionError> {
        self.insert(ConsKey::new((one, many)), ())
            .map(|removed| removed.is_none())
    }

    /// Returns `true` if relation was removed, i.e. it existed.
    pub fn remove_relation(
        &self,
        one: OneKey,
        many: ManyKey,
    ) -> Result<bool, UnabortableTransactionError> {
        self.remove(ConsKey::new((one, many)))
            .map(|removed| removed.is_some())
    }
}

pub type Primary<T, Codec> = TypedTree<UuidKey<T>, EncodedValue<T, Codec>>;

impl<T, Codec> Primary<T, Codec>
where
    Codec: DbValueCodec<T>,
{
    pub fn get_edge(&self, cursor: UuidKey<T>) -> TreeResult<Option<Edge<T>>> {
        let node = self.get(cursor).decode()?;
        Ok(node.map(|node| Edge { node, cursor }))
    }

    pub fn insert_edges(&self, edges: impl IntoIterator<Item = Edge<T>>) -> TreeResult<()> {
        for edge in edges.into_iter() {
            let encoded = edge.node.encode().map_err(TreeError::other)?;
            self.insert(edge.cursor, encoded)?;
        }
        Ok(())
    }

    pub fn add(&self, node: T) -> TreeResult<UuidKey<T>> {
        let encoded = node.encode().map_err(TreeError::other)?;
        let key = UuidKey::new_now();
        self.insert(key, encoded)?;
        Ok(key)
    }

    pub fn transact_add(
        &self,
        tx: &TypedTransactionalTree<UuidKey<T>, EncodedValue<T, Codec>>,
        node: &T,
    ) -> ConflictableTransactionResult<UuidKey<T>, Codec::EncodeError> {
        let encoded = node.encode().or_else(abort)?;
        let key = UuidKey::new_now();
        tx.insert(key, encoded)?;
        Ok(key)
    }

    pub fn transact_insert(
        &self,
        tx: &TypedTransactionalTree<UuidKey<T>, EncodedValue<T, Codec>>,
        key: UuidKey<T>,
        node: &T,
    ) -> ConflictableTransactionResult<UuidKey<T>, Codec::EncodeError> {
        let encoded = node.encode().or_else(abort)?;
        tx.insert(key, encoded)?;
        Ok(key)
    }
}

pub type SerdeTree<T, K = UuidKey<T>> = TypedTree<K, EncodedValue<T, SerdeJsonCodec>>;
pub type SerdeSingleton<T> = TypedTree<SingletonKey, EncodedValue<T, SerdeJsonCodec>>;
