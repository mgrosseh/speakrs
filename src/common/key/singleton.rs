use sled::IVec;

use crate::common::{
    codec::DbValueCodec,
    tree::{TreeResult, TypedTree},
};

/// A table key that has only one allowed value. Use when you only want to store a singular record of given data type.
#[derive(Clone, Copy)]
pub struct SingletonKey;

impl AsRef<[u8]> for SingletonKey {
    fn as_ref(&self) -> &[u8] {
        &[]
    }
}

impl From<SingletonKey> for IVec {
    fn from(_value: SingletonKey) -> Self {
        IVec::default()
    }
}

impl From<IVec> for SingletonKey {
    fn from(_value: IVec) -> Self {
        Self
    }
}

impl<V, Codec> TypedTree<SingletonKey, V, Codec>
where
    Codec: DbValueCodec<V>,
{
    pub fn has_single(&self) -> TreeResult<bool, V, Codec> {
        self.has_key(SingletonKey)
    }

    pub fn get_single(&self) -> TreeResult<Option<V>, V, Codec> {
        self.get(SingletonKey)
    }

    /// Insert a key to a new value
    pub fn set_single(&self, value: V) -> TreeResult<(), V, Codec> {
        self.set(SingletonKey, value)
    }

    /// Insert a key to a new value, returing the old value if present.
    #[allow(unused)] // TODO
    pub fn insert_replace_single(&self, value: V) -> TreeResult<Option<V>, V, Codec> {
        self.replace(SingletonKey, value)
    }
}
