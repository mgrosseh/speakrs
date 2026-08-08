use serde::{Deserialize, Serialize};
use sled::{IVec, transaction::TransactionalTree};
use std::marker::PhantomData;
use uuid::{Bytes, Timestamp, Uuid};

use crate::common::key::{
    UuidKey, UuidNowKeygen,
    generator::{DefaultContext, GenerateKey, KeyGenerator},
};

#[derive(Debug, Serialize, Deserialize)]
pub struct PrefixedKey<Prefix, T> {
    uuids: [Uuid; 2],
    id_type: PhantomData<(Prefix, T)>,
}

impl<T> UuidKey<T> {
    pub fn with_prefix<P>(self, prefix: UuidKey<P>) -> PrefixedKey<UuidKey<P>, T> {
        PrefixedKey {
            uuids: [prefix.uid, self.uid],
            id_type: PhantomData,
        }
    }
}
impl<Prefix, T> std::fmt::Display for PrefixedKey<Prefix, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "(")?;
        self.uuids[0].fmt(f)?;
        write!(f, ", ")?;
        self.uuids[1].fmt(f)?;
        write!(f, ")")
    }
}

impl<Prefix, T> Copy for PrefixedKey<Prefix, T> {}
impl<Prefix, T> Clone for PrefixedKey<Prefix, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<Prefix, T> PrefixedKey<Prefix, T> {
    pub fn suffix(&self) -> UuidKey<T> {
        UuidKey::with_uid(self.uuids[1])
    }
}

impl<P, T> PrefixedKey<UuidKey<P>, T> {
    pub fn new_now(prefix: UuidKey<P>) -> Self {
        UuidKey::<T>::new_now().with_prefix(prefix)
    }

    pub fn new_at_time(ts: Timestamp, prefix: UuidKey<P>) -> Self {
        UuidKey::<T>::new_at_time(ts).with_prefix(prefix)
    }

    pub fn prefix(&self) -> UuidKey<P> {
        UuidKey::with_uid(self.uuids[0])
    }
}

impl<Prefix, T> From<IVec> for PrefixedKey<Prefix, T> {
    /// Converts this type from the input type
    /// Panics if [`value.len()`] != 32
    fn from(value: IVec) -> Self {
        let bytes = value
            .as_array::<32>()
            .expect("Tried decoding a prefixed UUID of invalid byte length");
        let chunks: [Bytes; 2] = bytemuck::cast(*bytes);
        PrefixedKey {
            uuids: chunks.map(Uuid::from_bytes),
            id_type: PhantomData,
        }
    }
}

impl<Prefix, T> AsRef<[u8]> for PrefixedKey<Prefix, T> {
    fn as_ref(&self) -> &[u8] {
        bytemuck::bytes_of(&self.uuids)
    }
}

pub struct PrefixedKeygen<Prefix>(Prefix, UuidNowKeygen);
impl<Prefix> KeyGenerator<DefaultContext> for PrefixedKeygen<Prefix>
where
    PrefixedKeygen<Prefix>: Default,
{
    fn construct(_: DefaultContext, _: &TransactionalTree) -> Self {
        Default::default()
    }
}

impl<Prefix> KeyGenerator<Prefix> for PrefixedKeygen<Prefix>
where
    Prefix: Clone,
{
    fn construct(prefix: Prefix, tree: &TransactionalTree) -> Self {
        Self(
            prefix.clone(),
            UuidNowKeygen::construct(DefaultContext, tree),
        )
    }
}

impl<Prefix, NestedCtx> KeyGenerator<(Prefix, NestedCtx)> for PrefixedKeygen<Prefix>
where
    Prefix: Clone,
    UuidNowKeygen: KeyGenerator<NestedCtx>,
{
    fn construct((prefix, context): (Prefix, NestedCtx), tree: &TransactionalTree) -> Self {
        Self(prefix.clone(), UuidNowKeygen::construct(context, tree))
    }
}

impl<P, T> GenerateKey<PrefixedKey<UuidKey<P>, T>> for PrefixedKeygen<UuidKey<P>> {
    fn generate_next(&self) -> PrefixedKey<UuidKey<P>, T> {
        self.1.generate_next().with_prefix(self.0.clone())
    }
}
