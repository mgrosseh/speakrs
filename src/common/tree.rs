pub mod insertable;
pub mod iter;
pub mod subscriber;

use crate::common::{key::generator::KeyGenerator, tree::insertable::DbInsertable};

use super::{codec::DbValueCodec, key::KeyPrefix};
// use anyhow::Result;
use iter::DBIter;
use sled::{
    IVec, Tree,
    transaction::{
        ConflictableTransactionError, ConflictableTransactionResult, TransactionError,
        TransactionalTree,
    },
};
use std::{borrow::Borrow, marker::PhantomData, ops::RangeBounds};
use subscriber::DBSubscriber;
use tarpc::client::RpcError;

/// A "placeholder" key generator type that indicates no automatic key generation, i.e. key must always be explicitly provided.
pub struct KeyMustBeProvided;

#[derive(thiserror::Error, Debug)]
pub enum TreeError<EncodeError, DecodeError> {
    #[error("Internal database error: `{0}`")]
    Sled(#[from] sled::Error),
    #[error("Rpc error: `{0}`")]
    Rpc(#[from] RpcError),
    #[error("Error encoding value into database: `{0}`")]
    Encode(EncodeError),
    #[error("Error decoding value from database: `{0}`")]
    Decode(DecodeError),
}

type TreeErrorForCodec<V, C> =
    TreeError<<C as DbValueCodec<V>>::EncodeError, <C as DbValueCodec<V>>::DecodeError>;
pub type TreeResult<T, V, C> = std::result::Result<T, TreeErrorForCodec<V, C>>;

#[derive(Clone)]
pub struct DBTree<K, V, Codec, KeyGen = KeyMustBeProvided> {
    inner: Tree,
    _marker: PhantomData<(K, V, Codec, KeyGen)>,
}

impl<K, V, Codec, KeyGen> DBTree<K, V, Codec, KeyGen> {
    pub(super) fn from_raw(inner: Tree) -> Self {
        Self {
            inner,
            _marker: PhantomData,
        }
    }

    /// Returns the number of elements in this tree.
    ///
    /// Beware: performs a full O(n) scan under the hood.
    #[allow(unused)]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns `true` if the `Tree` contains no elements.
    #[allow(unused)]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Clears the `Tree`, removing all values.
    ///
    /// Note that this is not atomic.
    #[allow(unused)]
    pub fn clear(&self) -> sled::Result<()> {
        self.inner.clear()
    }

    /// Returns the CRC32 of all keys and values
    /// in this Tree.
    ///
    /// This is O(N) and locks the underlying tree
    /// for the duration of the entire scan.
    #[allow(unused)]
    pub fn checksum(&self) -> sled::Result<u32> {
        self.inner.checksum()
    }
}

impl<K, V, Codec, KeyGen> DBTree<K, V, Codec, KeyGen>
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

    // BUG: inserting a PrefixedKey does not work because of unfullfilled trait bounds. // TODO documentation, bugfix
    pub fn insert<InsertImpl>(
        &self,
        insertable: InsertImpl,
    ) -> TreeResult<InsertImpl::Return, V, Codec>
    where
        InsertImpl: DbInsertable<K, V, KeyGen>,
        KeyGen: KeyGenerator,
    {
        self.insert_in_context(Default::default(), insertable)
    }

    pub fn insert_in_context<Context, InsertImpl>(
        &self,
        context: Context,
        insertable: InsertImpl,
    ) -> TreeResult<InsertImpl::Return, V, Codec>
    where
        InsertImpl: DbInsertable<K, V, KeyGen>,
        KeyGen: KeyGenerator<Context>,
        Context: Clone,
    {
        self.transaction(|tree| {
            let generator = KeyGen::construct(context.clone(), tree);
            insertable.execute_insert(&generator, |key, value| {
                let encoded = Self::encode(value).map_err(ConflictableTransactionError::Abort)?;
                tree.insert(key.as_ref(), encoded)?;
                Ok(())
            })
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
    pub fn range(&self, range: impl RangeBounds<K>) -> DBIter<K, V, Codec, KeyGen> {
        DBIter {
            tree: DBTree::from_raw(self.inner.clone()),
            iter: self.inner.range(range),
        }
    }

    /// Create a double-ended iterator over the tuples of keys and
    /// values in this tree.
    pub fn iter(&self) -> DBIter<K, V, Codec, KeyGen> {
        DBIter {
            tree: DBTree::from_raw(self.inner.clone()),
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
    pub fn watch_all(&self) -> DBSubscriber<K, V, Codec, KeyGen> {
        DBSubscriber {
            tree: DBTree::from_raw(self.inner.clone()),
            inner: self.inner.watch_prefix(vec![]),
        }
    }

    // TODO: set merge opperation
    // TODO: pop_min, pop_max
    // TODO: batch translation layer
    // TODO: transaction translation layer
}

impl<K, V, Codec, KeyGen> DBTree<K, V, Codec, KeyGen>
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
    pub fn watch_partial<P>(&self, part: P) -> DBSubscriber<K, V, Codec, KeyGen>
    where
        P: KeyPrefix<K>,
    {
        DBSubscriber {
            tree: DBTree::from_raw(self.inner.clone()),
            inner: self.inner.watch_prefix(part.to_prefix()),
        }
    }
}

#[cfg(test)]
mod test {
    use sled::Db;

    use crate::common::{
        codec::PodCodec,
        key::integer::{IntegerKey, MonotonicKeygen},
        schema::{ChannelKey, MESSAGES_TABLE, MessageData, MessageKey, UserData, UserKey},
        table::SerdeTree,
    };

    use super::{subscriber::DBEvent, *};

    fn mock_db() -> Db {
        sled::Config::new().temporary(true).open().expect("open")
    }

    #[test]
    fn test_watch_all() -> anyhow::Result<()> {
        let db = mock_db();
        let decl = SerdeTree::<UserData>::decl("test_watch_all");
        let tree = decl.open(&db)?;
        let subscriber = tree.watch_all();

        let _thread = std::thread::spawn(move || {
            let tree = decl.open(&db).expect("open");
            tree.insert(UserData::new("TestUser1".to_owned()))
        });

        for event in subscriber.take(1) {
            match event {
                Ok(DBEvent::Insert { value, .. }) => assert_eq!(value.name.as_str(), "TestUser1"),
                Ok(DBEvent::Remove { .. }) => panic!("No remove should have been called!"),
                Err(e) => return Err(e),
            }
        }

        Ok(())
    }

    #[test]
    fn test_watch_partial() -> anyhow::Result<()> {
        let db = mock_db();
        let tree = MESSAGES_TABLE.open(&db).expect("open");

        let channel = ChannelKey::new_now();
        let message = MessageKey::new_now(channel);

        let subscriber = tree.watch_partial(channel);

        let _thread = std::thread::spawn(move || {
            let tree = MESSAGES_TABLE.open(&db).expect("open");
            tree.set(
                message,
                MessageData::now(UserKey::new_now(), "testing".to_owned()),
            )
        });

        for event in subscriber.take(1) {
            match event {
                Ok(DBEvent::Insert { value, key }) => {
                    assert_eq!(value.content.as_str(), "testing");
                    assert_eq!(key, message);
                }
                Ok(DBEvent::Remove { .. }) => panic!("No remove should have been called!"),
                Err(e) => return Err(e),
            }
        }

        Ok(())
    }

    #[test]
    fn test_autoincrement() -> anyhow::Result<()> {
        let db = mock_db();
        let decl = DBTree::<IntegerKey, i32, PodCodec, MonotonicKeygen>::decl("test_autoincrement");
        let table = decl.open(&db).expect("open");

        let key1 = table.insert(1).unwrap();
        let key2 = table.insert(2).unwrap();
        let key3 = table.insert(3).unwrap();
        std::assert_matches!(table.get(key1), Ok(Some(1)));
        std::assert_matches!(table.get(key2), Ok(Some(2)));
        std::assert_matches!(table.get(key3), Ok(Some(3)));
        Ok(())
    }

    #[test]
    fn test_insert_without_gen() -> anyhow::Result<()> {
        let db = mock_db();
        let decl = DBTree::<IntegerKey, u64, PodCodec>::decl("test_insert_without_gen");
        let table = decl.open(&db).expect("open");

        // Intentionally not possible, will fail at compile time:
        // table.insert(30).unwrap();

        table.set(IntegerKey(2), 20).unwrap();

        Ok(())
    }
}
