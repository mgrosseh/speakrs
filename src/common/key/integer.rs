use sled::{IVec, transaction::TransactionalTree};

use crate::common::key::generator::{DefaultContext, GenerateKey, KeyGenerator};

pub struct IntegerKey(pub u64);

impl AsRef<[u8]> for IntegerKey {
    fn as_ref(&self) -> &[u8] {
        bytemuck::bytes_of(&self.0)
    }
}

impl From<IntegerKey> for IVec {
    fn from(value: IntegerKey) -> Self {
        IVec::from(value.as_ref())
    }
}

impl From<IVec> for IntegerKey {
    fn from(value: IVec) -> Self {
        IntegerKey(u64::from_le_bytes(
            value.as_array().copied().expect("Invalid key byte length"),
        ))
    }
}

// Monotonic key generation using sled's built-in `generate_id`.
#[allow(unused)]
pub struct MonotonicKeygen(TransactionalTree);

impl KeyGenerator for MonotonicKeygen {
    fn construct(_: DefaultContext, tree: &TransactionalTree) -> Self {
        Self(tree.clone())
    }
}
impl GenerateKey<IntegerKey> for MonotonicKeygen {
    fn generate_next(&self) -> IntegerKey {
        IntegerKey(self.0.generate_id().unwrap())
    }
}
