use sled::{IVec, Iter};

use crate::common::codec::DbValueCodec;

use super::DBTree;


#[allow(unused)]
pub struct DBIter<K, V, Codec, KeyGen> {
    pub(super) tree: DBTree<K, V, Codec, KeyGen>,
    pub(super) iter: Iter,
}
impl<K, V, Codec, KeyGen> DBIter<K, V, Codec, KeyGen> {

}

impl<K, V, Codec, KeyGen> DBIter<K, V, Codec, KeyGen>
where
    K: AsRef<[u8]> + From<IVec> + Send + Sync,
    Codec: DbValueCodec<V> + Send + Sync,
    V: Send + Sync,
    KeyGen: Send + Sync, {
    /// Iterate over the keys of this Tree
    #[allow(unused)]
    pub fn keys(
        self,
    ) -> impl DoubleEndedIterator<Item = anyhow::Result<K>> + Send + Sync {
        self.map(|r| r.map(|(k, _v)| k))
    }

    /// Iterate over the values of this Tree
    #[allow(unused)]
    pub fn values(
        self,
    ) -> impl DoubleEndedIterator<Item = anyhow::Result<V>> + Send + Sync {
        self.map(|r| r.map(|(_k, v)| v))
    }
}

impl<K, V, Codec, KeyGen> Iterator for DBIter<K, V, Codec, KeyGen>
where
    K: AsRef<[u8]> + From<IVec>,
    Codec: DbValueCodec<V>, {
    type Item = anyhow::Result<(K, V)>;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(DBTree::<K, V, Codec, KeyGen>::decode_entry)
    }

    fn last(mut self) -> Option<Self::Item> {
        // TODO: double check
        self.iter.next_back().map(DBTree::<K, V, Codec, KeyGen>::decode_entry)
    }
}

impl<K, V, Codec, KeyGen> DoubleEndedIterator for DBIter<K, V, Codec, KeyGen>
where
    K: AsRef<[u8]> + From<IVec>,
    Codec: DbValueCodec<V>, {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.iter.next_back().map(DBTree::<K, V, Codec, KeyGen>::decode_entry)
    }
}
