use anyhow::Result;
use sled::{
    IVec, Tree,
    transaction::{ConflictableTransactionError, TransactionError},
};
use std::{borrow::Borrow, marker::PhantomData, ops::RangeBounds};

use crate::common::key::{GenerateKey, KeyGenerator, SingletonKey};

use super::codec::DbValueCodec;

pub struct DBTree<K, V, Codec> {
    inner: Tree,
    _marker: PhantomData<(K, V, Codec)>,
}

impl<K, V, Codec> DBTree<K, V, Codec> {
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

impl<K, V, Codec> DBTree<K, V, Codec>
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

    pub fn insert<Gen: KeyGenerator<KeyContext = ()>, InsertImpl: DbInsertable<K, V, Gen>>(
        &self,
        insertable: InsertImpl,
    ) -> Result<InsertImpl::Return> {
        self.insert_in_context(&(), insertable)
    }

    pub fn insert_in_context<Gen: KeyGenerator, InsertImpl: DbInsertable<K, V, Gen>>(
        &self,
        context: &Gen::KeyContext,
        insertable: InsertImpl,
    ) -> Result<InsertImpl::Return> {
        self.inner
            .transaction(|tree| {
                let generator = Gen::construct(context, tree);
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

/// All potential shapes for `tree.insert` argument.
pub trait DbInsertable<K, V, Gen: KeyGenerator>: Sized {
    type Return;
    fn execute_insert(
        &self,
        generator: &Gen,
        do_insert_entry: impl Fn(&K, &V) -> Result<()>,
    ) -> Result<Self::Return>;
}

// Implementation for direct key,value pair insertion.
impl<K, V> DbInsertable<K, V, ()> for (K, V)
where
    K: AsRef<[u8]> + From<IVec>,
{
    type Return = ();
    fn execute_insert(
        &self,
        _generator: &(),
        do_insert_entry: impl Fn(&K, &V) -> Result<()>,
    ) -> Result<Self::Return> {
        do_insert_entry(&self.0, &self.1)
    }
}

// Implementation for value insertion with autogenerated key.
impl<K, V, Gen: GenerateKey<K>> DbInsertable<K, V, Gen> for V
where
    K: AsRef<[u8]> + From<IVec>,
{
    type Return = K;
    fn execute_insert(
        &self,
        generator: &Gen,
        do_insert_entry: impl Fn(&K, &V) -> Result<()>,
    ) -> Result<Self::Return> {
        let key = generator.generate_next();
        do_insert_entry(&key, &self)?;
        Ok(key)
    }
}

impl<const N: usize, K, V, Gen: GenerateKey<K>> DbInsertable<K, V, Gen> for [V; N] {
    type Return = [K; N];

    fn execute_insert(
        &self,
        generator: &Gen,
        do_insert_entry: impl Fn(&K, &V) -> Result<()>,
    ) -> Result<Self::Return> {
        let keys = std::array::from_fn(|_| generator.generate_next());
        keys.as_slice()
            .iter()
            .zip(self.as_slice())
            .try_for_each(|(k, v)| do_insert_entry(k, v))?;
        Ok(keys)
    }
}

impl<K, V, Gen: GenerateKey<K>> DbInsertable<K, V, Gen> for &[V]
where
    V: DbInsertable<K, V, Gen>,
{
    type Return = Vec<V::Return>;

    fn execute_insert(
        &self,
        generator: &Gen,
        do_insert_entry: impl Fn(&K, &V) -> Result<()>,
    ) -> Result<Self::Return> {
        Ok(self
            .into_iter()
            .map(move |v| v.execute_insert(generator, &do_insert_entry))
            .collect::<Result<_, _>>()?)
    }
}

impl<V, Codec> DBTree<SingletonKey, V, Codec>
where
    Codec: DbValueCodec<V>,
{
    pub fn get_single(&self) -> Result<Option<V>> {
        Ok(self
            .inner
            .get(SingletonKey)?
            .map(Codec::decode_owned)
            .transpose()?)
    }

    /// Insert a key to a new value
    pub fn insert_single(&self, value: V) -> Result<()> {
        self.set(SingletonKey, value)
    }

    /// Insert a key to a new value, returing the old value if present.
    #[allow(unused)] // TODO
    pub fn insert_replace_single(&self, value: V) -> Result<Option<V>> {
        self.replace(SingletonKey, value)
    }
}
