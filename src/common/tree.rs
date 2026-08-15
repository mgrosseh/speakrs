pub mod insertable;
pub mod iter;
pub mod subscriber;

use crate::common::{key::generator::KeyGenerator, tree::insertable::DbInsertable};

use super::{codec::DbValueCodec, key::KeyPrefix};
use anyhow::Result;
use iter::DBIter;
use sled::{
    IVec, Tree, transaction::{ConflictableTransactionError, TransactionError}
};
use subscriber::DBSubscriber;
use std::{borrow::Borrow, marker::PhantomData, ops::RangeBounds};

/// A "placeholder" key generator type that indicates no automatic key generation, i.e. key must always be explicitly provided.
pub struct KeyMustBeProvided;

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
    pub fn set(&self, key: impl Borrow<K>, value: V) -> Result<()> {
        self.inner
            .insert(key.borrow(), Codec::encode_owned(value)?)?;
        Ok(())
    }

    // BUG: inserting a PrefixedKey does not work because of unfullfilled trait bounds. // TODO documentation, bugfix
    pub fn insert<InsertImpl>(&self, insertable: InsertImpl) -> Result<InsertImpl::Return>
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
    ) -> Result<InsertImpl::Return>
    where
        InsertImpl: DbInsertable<K, V, KeyGen>,
        KeyGen: KeyGenerator<Context>,
        Context: Clone,
    {
        self.inner
            .transaction(|tree| {
                let generator = KeyGen::construct(context.clone(), tree);
                insertable
                    .execute_insert(&generator, |key, value| {
                        tree.insert(key.as_ref(), Codec::encode(value)?)?;
                        Ok(())
                    })
                    .map_err(ConflictableTransactionError::Abort)
            })
            .map_err(|e| match e {
                TransactionError::Abort(error) => error,
                TransactionError::Storage(error) => error.into(),
            })
    }

    /// Insert a key to a new value, returing the old value if present.
    pub fn replace(&self, key: impl Borrow<K>, value: V) -> Result<Option<V>> {
        Ok(self
            .inner
            .insert(key.borrow(), Codec::encode_owned(value)?)?
            .map(Codec::decode_owned)
            .transpose()?)
    }

    /// Get a value corresponding to the key, or None if none.
    pub fn get(&self, key: K) -> Result<Option<V>> {
        Ok(self.inner.get(key)?.map(Codec::decode_owned).transpose()?)
    }

    /// Check if specific key exists.
    pub fn has_key(&self, key: K) -> Result<bool> {
        Ok(self.inner.get(key)?.is_some())
    }

    fn decode_key_value_pair((ikey, ival): (IVec, IVec)) -> Result<(K, V)> {
        Ok((K::from(ikey), Codec::decode_owned(ival)?))
    }

    fn decode_opt_entry(pair: Result<Option<(IVec, IVec)>, sled::Error>) -> Result<Option<(K, V)>> {
        pair?.map(Self::decode_key_value_pair).transpose()
    }

    fn decode_entry(pair: Result<(IVec, IVec), sled::Error>) -> Result<(K, V)> {
        Self::decode_key_value_pair(pair?)
    }

    /// Get the first key-value-pair in this tree.
    /// Keys are sorted by their bytes
    /// To retain the ordering of numerical types use big endian reprensentation
    #[allow(unused)] // TODO
    pub fn first(&self) -> Result<Option<(K, V)>> {
        Self::decode_opt_entry(self.inner.first())
    }
    /// Get the last key-value-pair in this tree.
    /// Keys are sorted by their bytes
    /// To retain the ordering of numerical types use big endian reprensentation
    pub fn last(&self) -> Result<Option<(K, V)>> {
        Self::decode_opt_entry(self.inner.last())
    }
    /// Get the next key-value-pair.
    /// That means, get K that using byte ordering is greater than [`key`], or none if [`key`] is the last key.
    /// Keys are sorted by their bytes.
    /// To retain the ordering of numerical types use big endian reprensentation.
    #[allow(unused)] // TODO
    pub fn next(&self, key: K) -> Result<Option<(K, V)>> {
        Self::decode_opt_entry(self.inner.get_gt(key))
    }

    /// Get the previous key-value-pair.
    /// That means, get K that using byte ordering is less than [`key`], or none if [`key`] is the first key.
    /// Keys are sorted by their bytes.
    /// To retain the ordering of numerical types use big endian reprensentation.
    #[allow(unused)] // TODO
    pub fn prev(&self, key: K) -> Result<Option<(K, V)>> {
        Self::decode_opt_entry(self.inner.get_lt(key))
    }

    /// Access a range of keys as an iterator.
    /// Keys are sorted by their bytes.
    /// To retain the ordering of numerical types use big endian reprensentation.
    pub fn range(&self, range: impl RangeBounds<K>) -> DBIter<K, V, Codec, KeyGen> {
        DBIter {
            tree: DBTree::from_raw(self.inner.clone()),
            iter: self.inner.range(range)
        }
    }

    /// Create a double-ended iterator over the tuples of keys and
    /// values in this tree.
    pub fn iter(&self) -> DBIter<K, V, Codec, KeyGen> {
        DBIter {
            tree: DBTree::from_raw(self.inner.clone()),
            iter: self.inner.iter()
        }
    }

    /// Returns a double ended iterator filtered by `filter`.
    /// If a value is Err, include_err decides whether to include or not.
    ///
    /// Convenience method for `iter().filter()`
    pub fn filter(&self, filter: impl Fn(&(K, V)) -> bool, include_err: bool) -> impl DoubleEndedIterator<Item = Result<(K, V)>> {
        self.iter().filter(move |result| match result {
            Ok(kv) => filter(kv),
            Err(_) => include_err,
        })
    }

    /// Searches for a value that satisfies a predicate.
    ///
    /// Convenience method for `iter().find()`
    pub fn find(&self, predicate: impl Fn(&(K, V)) -> bool) -> Option<Result<(K, V)>> {
        self.iter().find(move |result| match result {
            Ok(kv) => predicate(kv),
            Err(_) => true,
        })
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
    where P: KeyPrefix<K>, {
        DBSubscriber {
            tree: DBTree::from_raw(self.inner.clone()),
            inner: self.inner.watch_prefix(part.to_prefix())
        }
    }
}

// TODO: tests for watch

#[cfg(test)]
mod test {
    use sled::Db;

    use crate::common::{
        codec::PodCodec, key::integer::{IntegerKey, MonotonicKeygen}, schema::{ChannelKey, MESSAGES_TABLE, MessageData, MessageKey, UserData, UserKey}, table::SerdeTree
    };

    use super::{subscriber::DBEvent, *};

    fn mock_db() -> Db {
        sled::Config::new().temporary(true).open().expect("open")
    }

    #[test]
    fn test_watch_all() -> Result<()> {
        let db = mock_db();
        let decl = SerdeTree::<UserData>::decl("test_watch_all");
        let tree = decl.open(&db).expect("open");
        let subscriber = tree.watch_all();

        let _thread = std::thread::spawn(move || {
            let tree = decl.open(&db).expect("open");
            tree.insert(UserData::new("TestUser1".to_owned()))
        });

        for event in subscriber.take(1) {
            match event {
                Ok(DBEvent::Insert{ value, .. }) => assert_eq!(value.name.as_str(), "TestUser1"),
                Ok(DBEvent::Remove { .. }) => panic!("No remove should have been called!"),
                Err(e) => return Err(e)
            }
        }

        Ok(())
    }

    #[test]
    fn test_watch_partial() -> Result<()> {
        let db = mock_db();
        let tree = MESSAGES_TABLE.open(&db).expect("open");

        let channel = ChannelKey::new_now();
        let message = MessageKey::new_now(channel);

        let subscriber = tree.watch_partial(channel);

        let _thread = std::thread::spawn(move || {
            let tree = MESSAGES_TABLE.open(&db).expect("open");
            tree.set(message, MessageData::now(UserKey::new_now(), "testing".to_owned()))
        });

        for event in subscriber.take(1) {
            match event {
                Ok(DBEvent::Insert{ value, key }) => {
                    assert_eq!(value.content.as_str(), "testing");
                    assert_eq!(key, message);
                }
                Ok(DBEvent::Remove { .. }) => panic!("No remove should have been called!"),
                Err(e) => return Err(e)
            }
        }


        Ok(())
    }

    #[test]
    fn test_autoincrement() -> Result<()> {
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
    fn test_insert_without_gen() -> Result<()> {
        let db = mock_db();
        let decl = DBTree::<IntegerKey, u64, PodCodec>::decl("test_insert_without_gen");
        let table = decl.open(&db).expect("open");

        // Intentionally not possible, will fail at compile time:
        // table.insert(30).unwrap();

        table.set(IntegerKey(2), 20).unwrap();

        Ok(())
    }
}
