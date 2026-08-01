use anyhow::Result;
use sled::{IVec, Tree};
use std::{marker::PhantomData, ops::RangeBounds};

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
}

impl<K, V, Codec> DBTree<K, V, Codec>
where
    K: AsRef<[u8]> + From<IVec>,
    Codec: super::codec::DbValueCodec<V>,
{
    pub fn insert(&self, key: K, value: V) -> Result<()> {
        self.inner.insert(key, Codec::encode_owned(value)?)?;
        Ok(())
    }

    pub fn insert_replace(&self, key: K, value: V) -> Result<Option<V>> {
        Ok(self
            .inner
            .insert(key, Codec::encode_owned(value)?)?
            .map(Codec::decode_owned)
            .transpose()?)
    }

    pub fn get(&self, key: K) -> Result<Option<V>> {
        Ok(self.inner.get(key)?.map(Codec::decode_owned).transpose()?)
    }

    fn decode_key_value_pair((ikey, ival): (IVec, IVec)) -> Result<(K, V)> {
        Ok((K::from(ikey), Codec::decode(&ival)?))
    }

    fn decode_opt_entry(pair: Result<Option<(IVec, IVec)>, sled::Error>) -> Result<Option<(K, V)>> {
        pair?.map(Self::decode_key_value_pair).transpose()
    }

    fn decode_entry(pair: Result<(IVec, IVec), sled::Error>) -> Result<(K, V)> {
        Self::decode_key_value_pair(pair?)
    }

    pub fn first(&self) -> Result<Option<(K, V)>> {
        Self::decode_opt_entry(self.inner.first())
    }
    pub fn last(&self) -> Result<Option<(K, V)>> {
        Self::decode_opt_entry(self.inner.last())
    }

    pub fn next(&self, key: K) -> Result<Option<(K, V)>> {
        Self::decode_opt_entry(self.inner.get_gt(key))
    }

    pub fn prev(&self, key: K) -> Result<Option<(K, V)>> {
        Self::decode_opt_entry(self.inner.get_lt(key))
    }

    /// Access a range of keys as an iterator.
    pub fn range(&self, range: impl RangeBounds<K>) -> impl Iterator<Item = Result<(K, V)>> {
        self.inner.range(range).map(Self::decode_entry)
    }

    // TODO: iter translation layer
    // TODO: batch translation layer
    // TODO: transaction translation layer
}
