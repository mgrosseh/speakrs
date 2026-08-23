pub mod iter;
pub mod subscriber;

use crate::common::{
    codec::EncodedValue,
    key::{
        DbKey, Prefixed,
        generator::{DefaultContext, KeyGenerator},
    },
};

use super::codec::DbValueCodec;
// use anyhow::Result;
use iter::TreeIter;
use sled::{
    IVec, Transactional, Tree,
    transaction::{ConflictableTransactionResult, TransactionError, TransactionalTree},
};
use sled::{Result as SledResult, transaction::UnabortableTransactionError};
use std::{borrow::Borrow, marker::PhantomData, ops::RangeBounds};
use subscriber::DBSubscriber;

/// A "placeholder" key generator type that indicates no automatic key generation, i.e. key must always be explicitly provided.
pub struct KeyMustBeProvided;

impl KeyGenerator for KeyMustBeProvided {
    fn construct(_context: DefaultContext, _tree: &TransactionalTree) -> Self {
        KeyMustBeProvided
    }
}

#[derive(thiserror::Error, Debug)]
pub enum TreeError<EncodeError, DecodeError> {
    #[error("Internal database error: `{0}`")]
    Sled(#[from] sled::Error),
    // #[error("Rpc error: `{0}`")]
    // Rpc(#[from] RpcError),
    #[error("Error encoding value into database: `{0}`")]
    Encode(EncodeError),
    #[error("Error decoding value from database: `{0}`")]
    Decode(DecodeError),
}

type TreeErrorForCodec<V, C> =
    TreeError<<C as DbValueCodec<V>>::EncodeError, <C as DbValueCodec<V>>::DecodeError>;
pub type TreeResult<T, V, C> = std::result::Result<T, TreeErrorForCodec<V, C>>;

trait ITreeAccess<K, V> {
    type Encoded;

    /// Returns the number of elements in this tree.
    ///
    /// Beware: performs a full O(n) scan under the hood.
    #[allow(unused)]
    fn len(&self) -> usize;

    /// Returns `true` if the `Tree` contains no elements.
    #[allow(unused)]
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Check if specific key exists in the tree.
    ///
    /// Note: Using this outside of transactions is dangerous, since it can lead to Time-of-check to time-of-use type bugs.
    /// Remember that the value for given key can be inserted at any moment by another tread. If this check is important
    /// for correctness, make sure to use transactional access.
    fn has_key(&self, key: impl Borrow<K>) -> SledResult<bool> {
        Ok(self.get(key)?.is_some())
    }

    /// Get a value corresponding to the key, or [`None`] if no value is present.
    fn get(&self, key: impl Borrow<K>) -> SledResult<Option<Self::Encoded>>;

    /// Inserts an already encoded key-value pair into the tree.
    ///
    /// If the tree did not have this key present, [`None`] is returned.
    ///
    /// If the tree did have this key present, the value is updated, and the old value is returned.
    fn insert(&self, key: K, value: Self::Encoded) -> SledResult<Option<Self::Encoded>>;
}

/// Thin abstraction over [`sled::Tree`] with strongly typed key and value.
pub struct TypedTree<K, V, Codec> {
    inner: Tree,
    marker: PhantomData<(K, V, Codec)>,
}

impl<K, V, Codec> Clone for TypedTree<K, V, Codec> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            marker: PhantomData,
        }
    }
}

pub struct TypedTransactionalTree<K, V, Codec> {
    inner: TransactionalTree,
    marker: PhantomData<(K, V, Codec)>,
}

impl<K, V, Codec> Clone for TypedTransactionalTree<K, V, Codec> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            marker: PhantomData,
        }
    }
}

impl<K, V, Codec> TypedTree<K, V, Codec> {
    pub(super) fn from_raw(inner: Tree) -> Self {
        Self {
            inner,
            marker: PhantomData,
        }
    }
}

impl<K, V, Codec> ITreeAccess<K, V> for TypedTree<K, V, Codec>
where
    K: DbKey,
    Codec: DbValueCodec<V>,
{
    type Encoded = EncodedValue<V, Codec>;

    fn len(&self) -> usize {
        self.inner.len()
    }

    fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    fn get(&self, key: impl Borrow<K>) -> SledResult<Option<Self::Encoded>> {
        self.inner
            .get(key.borrow())
            .map(EncodedValue::from_raw_option)
    }

    fn insert(&self, key: K, value: Self::Encoded) -> SledResult<Option<Self::Encoded>> {
        self.inner
            .insert(key, value.raw)
            .map(EncodedValue::from_raw_option)
    }
}

impl<E, K, V, Codec> Transactional<E> for TypedTree<K, V, Codec> {
    type View = TypedTransactionalTree<K, V, Codec>;

    fn make_overlay(&self) -> SledResult<sled::transaction::TransactionalTrees> {
        <Tree as Transactional<E>>::make_overlay(&self.inner)
    }

    fn view_overlay(overlay: &sled::transaction::TransactionalTrees) -> Self::View {
        TypedTransactionalTree::from_raw(<Tree as Transactional<E>>::view_overlay(overlay))
    }
}

impl<E, K, V, Codec> Transactional<E> for &TypedTree<K, V, Codec> {
    type View = TypedTransactionalTree<K, V, Codec>;

    fn make_overlay(&self) -> SledResult<sled::transaction::TransactionalTrees> {
        <&Tree as Transactional<E>>::make_overlay(&&self.inner)
    }

    fn view_overlay(overlay: &sled::transaction::TransactionalTrees) -> Self::View {
        TypedTransactionalTree::from_raw(<&Tree as Transactional<E>>::view_overlay(overlay))
    }
}

/// Workaround needed to implementint [`sled::Transactional<E>`] on tuples of our own tree type.
///
/// Without this extra wrapper type, orphan rule prevents us from implementing `Transactional<E>` on `TypedTree` tuples without binding to `E` generic.
///
/// Usage:
///
/// ```ignore
/// Tx((tree1, tree2)).transaction(|(ttree1, ttree2|| {
///    // ...
/// })
/// ```
pub struct Tx<TypedTrees>(pub TypedTrees);

/// Helper trait that makes `impl_transactional_for_tx` macro significantly easier to implement.
/// Without this, the `Transactional<Err>` `impl` block would require separate instances of `K`, `V`, and `Codec` generics for each tuple element.
trait ToTransactional {
    type View;
    fn raw(&self) -> &Tree;
    fn wrap_view(view: TransactionalTree) -> Self::View;
}

impl<K, V, Codec> ToTransactional for TypedTree<K, V, Codec> {
    type View = TypedTransactionalTree<K, V, Codec>;
    fn raw(&self) -> &Tree {
        &self.inner
    }
    fn wrap_view(view: TransactionalTree) -> Self::View {
        TypedTransactionalTree::from_raw(view)
    }
}

macro_rules! impl_transactional_for_tx {
    (@tree $_t:ident) => { &Tree };
    ($head:ident $($tail:ident)*) => {
        impl_transactional_for_tx!($($tail)*);

        #[allow(unused_parens)]
        impl<Err, $head, $($tail,)*> Transactional<Err> for Tx<(&$head $(, &$tail)*)>
        where
            $head: ToTransactional,
            $($tail: ToTransactional,)*
        {
            type View = ($head::View  $(, $tail::View)*);
            fn make_overlay(&self) -> SledResult<sled::transaction::TransactionalTrees> {
                match self {
                    Tx(($head $(, $tail)*)) => {
                        let raw_trees = ($head.raw() $(, $tail.raw())*);
                        <(&Tree $(, impl_transactional_for_tx!(@tree $tail))*) as Transactional<Err>>::make_overlay(&raw_trees)
                    }
                }
            }

            fn view_overlay(overlay: &sled::transaction::TransactionalTrees) -> Self::View {
                let ($head $(, $tail)*) = <(&Tree $(, impl_transactional_for_tx!(@tree $tail))*) as Transactional<Err>>::view_overlay(overlay);
                ($head::wrap_view($head) $(, $tail::wrap_view($tail))*)
            }

        }
    };
    () => {};
}

// Implemented to the same tuple arity as `Transactional<Err> for (&Tree, ...)` in sled.
impl_transactional_for_tx!(A B C D E F G H I J K L M N);

impl<K, V, Codec> TypedTransactionalTree<K, V, Codec> {
    pub(super) fn from_raw(inner: TransactionalTree) -> Self {
        Self {
            inner,
            marker: PhantomData,
        }
    }
}

impl<K, V, Codec> TypedTransactionalTree<K, V, Codec>
where
    K: DbKey,
    Codec: DbValueCodec<V>,
{
    fn get(
        &self,
        key: impl Borrow<K>,
    ) -> Result<Option<EncodedValue<V, Codec>>, UnabortableTransactionError> {
        self.inner
            .get(key.borrow())
            .map(EncodedValue::from_raw_option)
    }

    fn insert(
        &self,
        key: K,
        value: EncodedValue<V, Codec>,
    ) -> Result<Option<EncodedValue<V, Codec>>, UnabortableTransactionError> {
        self.inner
            .insert(key, value.raw)
            .map(EncodedValue::from_raw_option)
    }
}

impl<K, V, Codec> TypedTree<K, V, Codec>
where
    K: AsRef<[u8]> + From<IVec>,
    Codec: DbValueCodec<V>,
{
    /// Insert a key to a new value
    pub fn set(&self, key: impl Borrow<K>, value: V) -> TreeResult<(), V, Codec> {
        self.inner
            .insert(key.borrow(), Self::encode_owned(value)?)?;
        Ok(())
    }

    /// Helper method delegating to [`Codec::encode_owned`] and mapping the error
    fn encode_owned(value: V) -> TreeResult<IVec, V, Codec> {
        Ok(Codec::encode_owned(value).map_err(TreeError::Encode)?)
    }

    /// Helper method delegating to [`Codec::encode`] and mapping the error
    fn encode(value: &V) -> TreeResult<IVec, V, Codec> {
        Ok(Codec::encode(value).map_err(TreeError::Encode)?)
    }

    /// Helper method delegating to [`Codec::decode_owned`] and mapping the error
    fn decode_owned(ivec: IVec) -> TreeResult<V, V, Codec> {
        Ok(Codec::decode_owned(ivec).map_err(TreeError::Decode)?)
    }

    fn transaction<F, A>(&self, f: F) -> TreeResult<A, V, Codec>
    where
        F: Fn(&TransactionalTree) -> ConflictableTransactionResult<A, TreeErrorForCodec<V, Codec>>,
    {
        self.inner.transaction(f).map_err(|e| match e {
            TransactionError::Abort(error) => error,
            TransactionError::Storage(error) => error.into(),
        })
    }

    /// Insert a key to a new value, returing the old value if present.
    pub fn replace(&self, key: impl Borrow<K>, value: V) -> TreeResult<Option<V>, V, Codec> {
        Ok(self
            .inner
            .insert(key.borrow(), Self::encode_owned(value)?)?
            .map(Self::decode_owned)
            .transpose()?)
    }

    /// Get a value corresponding to the key, or None if none.
    pub fn get(&self, key: K) -> TreeResult<Option<V>, V, Codec> {
        Ok(self.inner.get(key)?.map(Self::decode_owned).transpose()?)
    }

    /// Check if specific key exists.
    pub fn has_key(&self, key: K) -> TreeResult<bool, V, Codec> {
        Ok(self.inner.get(key)?.is_some())
    }

    fn decode_key_value_pair((ikey, ival): (IVec, IVec)) -> TreeResult<(K, V), V, Codec> {
        Ok((K::from(ikey), Self::decode_owned(ival)?))
    }

    fn decode_opt_entry(
        pair: Result<Option<(IVec, IVec)>, sled::Error>,
    ) -> TreeResult<Option<(K, V)>, V, Codec> {
        pair?.map(Self::decode_key_value_pair).transpose()
    }

    fn decode_entry(pair: Result<(IVec, IVec), sled::Error>) -> TreeResult<(K, V), V, Codec> {
        Self::decode_key_value_pair(pair?)
    }

    /// Get the first key-value-pair in this tree.
    /// Keys are sorted by their bytes
    /// To retain the ordering of numerical types use big endian reprensentation
    #[allow(unused)] // TODO
    pub fn first(&self) -> TreeResult<Option<(K, V)>, V, Codec> {
        Self::decode_opt_entry(self.inner.first())
    }
    /// Get the last key-value-pair in this tree.
    /// Keys are sorted by their bytes
    /// To retain the ordering of numerical types use big endian reprensentation
    pub fn last(&self) -> TreeResult<Option<(K, V)>, V, Codec> {
        Self::decode_opt_entry(self.inner.last())
    }
    /// Get the next key-value-pair.
    /// That means, get K that using byte ordering is greater than [`key`], or none if [`key`] is the last key.
    /// Keys are sorted by their bytes.
    /// To retain the ordering of numerical types use big endian reprensentation.
    #[allow(unused)] // TODO
    pub fn next(&self, key: K) -> TreeResult<Option<(K, V)>, V, Codec> {
        Self::decode_opt_entry(self.inner.get_gt(key))
    }

    /// Get the previous key-value-pair.
    /// That means, get K that using byte ordering is less than [`key`], or none if [`key`] is the first key.
    /// Keys are sorted by their bytes.
    /// To retain the ordering of numerical types use big endian reprensentation.
    #[allow(unused)] // TODO
    pub fn prev(&self, key: K) -> TreeResult<Option<(K, V)>, V, Codec> {
        Self::decode_opt_entry(self.inner.get_lt(key))
    }

    /// Access a range of keys as an iterator.
    /// Keys are sorted by their bytes.
    /// To retain the ordering of numerical types use big endian reprensentation.
    pub fn range(&self, range: impl RangeBounds<K>) -> TreeIter<K, V, Codec> {
        TreeIter {
            tree: self.clone(),
            iter: self.inner.range(range),
        }
    }

    /// Create a double-ended iterator over the tuples of keys and
    /// values in this tree.
    pub fn iter(&self) -> TreeIter<K, V, Codec> {
        TreeIter {
            tree: Self::from_raw(self.inner.clone()),
            iter: self.inner.iter(),
        }
    }

    /// Returns a double ended iterator filtered by `filter`.
    ///
    /// Convenience method for `iter().filter()`
    #[allow(unused)]
    pub fn try_filter(
        &self,
        filter: impl Fn(&(K, V)) -> bool,
    ) -> impl DoubleEndedIterator<Item = TreeResult<(K, V), V, Codec>> {
        self.iter().filter(move |result| match result {
            Ok(kv) => filter(kv),
            Err(_) => true,
        })
    }

    /// Searches for a value that satisfies a predicate.
    ///
    /// Convenience method for `iter().find()`
    pub fn try_find(
        &self,
        predicate: impl Fn(&(K, V)) -> bool,
    ) -> Option<TreeResult<(K, V), V, Codec>> {
        self.iter().find(move |result| match result {
            Ok(kv) => predicate(kv),
            Err(_) => true,
        })
    }

    /// Takes a closure and creates an iterator which calls that closure on each
    /// element.
    ///
    /// Convenience method for `iter().map()`
    pub fn map<T>(
        &self,
        mut map_fn: impl FnMut((K, V)) -> T,
    ) -> impl DoubleEndedIterator<Item = TreeResult<T, V, Codec>> {
        self.iter().map(move |result| result.map(|v| map_fn(v)))
    }

    /// Subscribe to `DBEvent`s that happen to all keys.
    /// `DBEvents` for particular keys are guaranteed to be
    /// witnessed in the same order by all threads, but
    /// threads may witness different interleavings of
    /// `DBEvents` across different keys. If subscribers don't
    /// keep up with new writes, they will cause new writes
    /// to block. There is a buffer of 1024 items per
    /// `DBSubscriber`. This can be used to build reactive
    /// and replicated systems.
    ///
    /// `DBSubscriber` implements both `Iterator<Item = Result<DBEvent>>`
    /// and `Future<Output=Option<Event>>`
    #[allow(unused)]
    pub fn watch_all(&self) -> DBSubscriber<K, V, Codec> {
        DBSubscriber {
            tree: self.clone(),
            inner: self.inner.watch_prefix(vec![]),
        }
    }

    // TODO: set merge opperation
    // TODO: pop_min, pop_max
    // TODO: batch translation layer
    // TODO: transaction translation layer
}

impl<K, V, Codec> TypedTree<K, V, Codec>
where
    K: AsRef<[u8]> + From<IVec>,
    Codec: DbValueCodec<V>,
{
    // NOTE: below might pick up data not actually part of the intended prefix, since we type our prefix in a particular way.
    // If there ever are other key schemes, where one key might contain part of another without them being related (e.g. strings).
    // I've given this some thought and think its very unlikely to ever be a problem, but theoretically could.
    /// Subscribe to `DBEvent`s that happen to keys starting
    /// with `part`. `DBEvents` for particular keys are
    /// guaranteed to be witnessed in the same order by all
    /// threads, but threads may witness different interleavings
    /// of `DBEvents` across different keys. If subscribers don't
    /// keep up with new writes, they will cause new writes
    /// to block. There is a buffer of 1024 items per
    /// `DBSubscriber`. This can be used to build reactive
    /// and replicated systems.
    ///
    /// `DBSubscriber` implements both `Iterator<Item = Result<DBEvent>>`
    /// and `Future<Output=Option<Event>>`
    #[allow(unused)]
    pub fn watch_partial<P>(&self, part: P) -> DBSubscriber<K, V, Codec>
    where
        K: Prefixed<P>,
        P: Into<IVec>,
    {
        DBSubscriber {
            tree: self.clone(),
            inner: self.inner.watch_prefix(part.into()),
        }
    }
}

#[cfg(test)]
mod test {
    use sled::Db;

    use crate::common::{
        codec::PodCodec,
        key::{compound::ConsKey, integer::IntegerKey},
        schema::{
            ChannelKey, MESSAGES_IN_CHANNEL_TABLE, MESSAGES_TABLE, MessageData, MessageKey, UserKey,
        },
    };

    use super::{subscriber::DBEvent, *};

    fn mock_db() -> Db {
        sled::Config::new().temporary(true).open().expect("open")
    }

    // #[test]
    // fn test_watch_all() -> anyhow::Result<()> {
    //     let db = mock_db();
    //     let decl = SerdeTree::<UserData>::decl("test_watch_all");
    //     let tree = decl.open(&db)?;
    //     let subscriber = tree.watch_all();

    //     let thread = std::thread::spawn(move || {
    //         let tree = decl.open(&db).expect("open");
    //         tree.insert(UserData::new("TestUser1".to_owned()))
    //     });

    //     for event in subscriber.take(1) {
    //         match event {
    //             Ok(DBEvent::Insert { value, .. }) => assert_eq!(value.name.as_str(), "TestUser1"),
    //             Ok(DBEvent::Remove { .. }) => panic!("No remove should have been called!"),
    //             Err(e) => return Err(e),
    //         }
    //     }

    //     thread.join().unwrap()?;
    //     Ok(())
    // }

    #[test]
    fn test_watch_partial() -> anyhow::Result<()> {
        let db = mock_db();
        let tree = MESSAGES_TABLE.open(&db)?;
        let relationship = MESSAGES_IN_CHANNEL_TABLE.open(&db)?;

        let channel = ChannelKey::new_now();
        let message = MessageKey::new_now();

        let subscriber = relationship.watch_partial(ConsKey::of(channel));

        let thread = std::thread::spawn(move || {
            let tree = MESSAGES_TABLE.open(&db)?;
            let relationship = MESSAGES_IN_CHANNEL_TABLE.open(&db)?;
            tree.set(
                message,
                MessageData::now(UserKey::new_now(), channel, "testing".to_owned()),
            )?;
            relationship.set(ConsKey::new((channel, message)), ())?;
            Ok::<_, anyhow::Error>(())
        });

        for event in subscriber.take(1) {
            match event {
                Ok(DBEvent::Insert { value, key }) => {
                    assert_eq!(value, ());
                    assert_eq!(key, ConsKey::new((channel, message)));
                    assert_eq!(
                        tree.get(message)?.map(|msg| msg.content),
                        Some("testing".to_owned())
                    )
                }
                Ok(DBEvent::Remove { .. }) => panic!("No remove should have been called!"),
                Err(e) => return Err(e),
            }
        }

        thread.join().unwrap()?;
        Ok(())
    }

    // #[test]
    // fn test_autoincrement() -> anyhow::Result<()> {
    //     let db = mock_db();
    //     let decl =
    //         TypedTree::<IntegerKey, i32, PodCodec, MonotonicKeygen>::decl("test_autoincrement");
    //     let table = decl.open(&db).expect("open");

    //     let key1 = table.insert(1).unwrap();
    //     let key2 = table.insert(2).unwrap();
    //     let key3 = table.insert(3).unwrap();
    //     std::assert_matches!(table.get(key1), Ok(Some(1)));
    //     std::assert_matches!(table.get(key2), Ok(Some(2)));
    //     std::assert_matches!(table.get(key3), Ok(Some(3)));
    //     Ok(())
    // }

    #[test]
    fn test_insert_without_gen() -> anyhow::Result<()> {
        let db = mock_db();
        let decl = TypedTree::<IntegerKey, u64, PodCodec>::decl("test_insert_without_gen");
        let table = decl.open(&db).expect("open");

        // Intentionally not possible, will fail at compile time:
        // table.insert(30).unwrap();

        table.set(IntegerKey(2), 20).unwrap();

        Ok(())
    }
}
