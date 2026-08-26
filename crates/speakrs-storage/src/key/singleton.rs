use sled::IVec;

use crate::{codec::Decodable, tree::TypedTree};

/// A table key that has only one allowed value. Use when you only want to store a singular record of given data type.
#[derive(Clone, Copy)]
pub struct SingletonKey(());

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
        Self(())
    }
}

impl<Encoded: Decodable> TypedTree<SingletonKey, Encoded> {
    pub fn get_single(&self) -> sled::Result<Option<Encoded>> {
        self.get(SingletonKey(()))
    }

    /// Insert a key to a new value
    pub fn insert_single(&self, value: Encoded) -> sled::Result<Option<Encoded>> {
        self.insert(SingletonKey(()), value)
    }

    #[allow(unused)]
    fn remove_single(&self) -> sled::Result<Option<Encoded>> {
        self.remove(SingletonKey(()))
    }
}
