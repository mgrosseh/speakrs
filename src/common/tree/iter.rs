use sled::{IVec, Iter};

use crate::common::{codec::DbValueCodec, tree::TreeResult};

use super::TypedTree;

#[allow(unused)]
pub struct TreeIter<K, V, Codec> {
    pub(super) tree: TypedTree<K, V, Codec>,
    pub(super) iter: Iter,
}
impl<K, V, Codec> TreeIter<K, V, Codec> {}

impl<K, V, Codec> TreeIter<K, V, Codec>
where
    K: AsRef<[u8]> + From<IVec> + Send + Sync,
    Codec: DbValueCodec<V> + Send + Sync,
    V: Send + Sync,
{
    /// Iterate over the keys of this Tree
    #[allow(unused)]
    pub fn keys(self) -> impl DoubleEndedIterator<Item = TreeResult<K, V, Codec>> + Send + Sync {
        self.map(|r| r.map(|(k, _v)| k))
    }

    /// Iterate over the values of this Tree
    #[allow(unused)]
    pub fn values(self) -> impl DoubleEndedIterator<Item = TreeResult<V, V, Codec>> + Send + Sync {
        self.map(|r| r.map(|(_k, v)| v))
    }
}

impl<K, V, Codec> Iterator for TreeIter<K, V, Codec>
where
    K: AsRef<[u8]> + From<IVec>,
    Codec: DbValueCodec<V>,
{
    type Item = TreeResult<(K, V), V, Codec>;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(TypedTree::<K, V, Codec>::decode_entry)
    }

    fn last(mut self) -> Option<Self::Item> {
        // TODO: double check
        self.iter
            .next_back()
            .map(TypedTree::<K, V, Codec>::decode_entry)
    }
}

impl<K, V, Codec> DoubleEndedIterator for TreeIter<K, V, Codec>
where
    K: AsRef<[u8]> + From<IVec>,
    Codec: DbValueCodec<V>,
{
    fn next_back(&mut self) -> Option<Self::Item> {
        self.iter
            .next_back()
            .map(TypedTree::<K, V, Codec>::decode_entry)
    }
}
