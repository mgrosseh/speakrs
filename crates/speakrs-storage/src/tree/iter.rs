use std::marker::PhantomData;

use sled::Iter;

use crate::{
    codec::Decodable,
    key::DbKey,
    tree::{TreeError, TreeResult},
};

use super::TypedTree;

pub struct TypedTreeIter<K, Encoded> {
    pub(super) iter: Iter,
    pub(super) marker: PhantomData<(K, Encoded)>,
}
impl<K, Encoded> TypedTreeIter<K, Encoded>
where
    K: DbKey,
    Encoded: Decodable,
{
    pub fn decode(self) -> impl Iterator<Item = TreeResult<(K, Encoded::Decoded)>> {
        self.map(|result| {
            let (key, encoded) = result?;
            let pair = (key, encoded.decode().map_err(TreeError::other)?);

            Ok(pair)
        })
    }
}

impl<K, Encoded> Iterator for TypedTreeIter<K, Encoded>
where
    K: DbKey,
    Encoded: Decodable,
{
    type Item = sled::Result<(K, Encoded)>;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(TypedTree::wrap_res_entry)
    }

    fn last(self) -> Option<Self::Item> {
        self.iter.last().map(TypedTree::wrap_res_entry)
    }
}

impl<K, Encoded> DoubleEndedIterator for TypedTreeIter<K, Encoded>
where
    K: DbKey,
    Encoded: Decodable,
{
    fn next_back(&mut self) -> Option<Self::Item> {
        self.iter.next_back().map(TypedTree::wrap_res_entry)
    }
}
