pub mod insertable;

use crate::common::{key::generator::KeyGenerator, tree::insertable::DbInsertable};

use super::codec::DbValueCodec;
use anyhow::Result;
use sled::{
    IVec, Tree,
    transaction::{ConflictableTransactionError, TransactionError},
};
use std::{borrow::Borrow, marker::PhantomData, ops::RangeBounds};

/// A "placeholder" key generator type that indicates no automatic key generation, i.e. key must always be explicitly provided.
pub struct KeyMustBeProvided;

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
    pub fn len(&self) -> usize {
        self.inner.len()
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
    #[allow(unused)] // TODO
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
    pub fn range(&self, range: impl RangeBounds<K>) -> impl Iterator<Item = Result<(K, V)>> {
        self.inner.range(range).map(Self::decode_entry)
    }

    // TODO: iter translation layer
    // TODO: batch translation layer
    // TODO: transaction translation layer
}

#[cfg(test)]
mod test {
    use sled::Db;

    use crate::common::{
        codec::PodCodec,
        key::integer::{IntegerKey, MonotonicKeygen},
    };

    use super::*;

    fn mock_db() -> Db {
        sled::Config::new().temporary(true).open().expect("open")
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
