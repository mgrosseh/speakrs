use anyhow::Result;
use sled::IVec;

use crate::common::{codec::DbValueCodec, tree::DBTree};

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

impl<V, Codec, Gen> DBTree<SingletonKey, V, Codec, Gen>
where
    Codec: DbValueCodec<V>,
{
    pub fn has_single(&self) -> Result<bool> {
        self.has_key(SingletonKey)
    }

    pub fn get_single(&self) -> Result<Option<V>> {
        self.get(SingletonKey)
    }

    /// Insert a key to a new value
    pub fn set_single(&self, value: V) -> Result<()> {
        self.set(SingletonKey, value)
    }

    /// Insert a key to a new value, returing the old value if present.
    #[allow(unused)] // TODO
    pub fn insert_replace_single(&self, value: V) -> Result<Option<V>> {
        self.replace(SingletonKey, value)
    }
}
