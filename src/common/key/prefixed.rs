use serde::{Deserialize, Serialize};
use sled::{IVec, transaction::TransactionalTree};
use std::marker::PhantomData;
use uuid::{Bytes, Timestamp, Uuid};

use crate::common::key::{
    UuidKey, UuidNowKeygen,
    generator::{DefaultContext, GenerateKey, KeyGenerator},
};

#[derive(Serialize, Deserialize)]
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
impl<Prefix, T> std::fmt::Debug for PrefixedKey<Prefix, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "(")?;
        self.uuids[0].fmt(f)?;
        write!(f, ", ")?;
        self.uuids[1].fmt(f)?;
        write!(f, ")")
    }
}

impl<Prefix, T> std::hash::Hash for PrefixedKey<Prefix, T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let lhs = self.uuids[0].as_u128();
        let rhs = self.uuids[1].as_u128();
        // use hash combining method that is a simplified version of boost::hash_combine
        // (I did not come up with this but looked into boost::hash_combine to double check)
        // The constant, however, has been replaced with a larger number: supposedly using the
        // expansion of pi, which I could not verify the methodology of, but its sufficient that
        // it is an odd large "noisy" number.
        //
        // Reference: https://stackoverflow.com/a/27952689
        let hash = lhs ^ (rhs + 0x517cc1b727220a95 + (lhs << 6) + (lhs >> 2));
        state.write_u128(hash);
    }
}

impl<Prefix, T> PartialEq for PrefixedKey<Prefix, T> {
    fn eq(&self, other: &Self) -> bool {
        self.uuids[0].eq(&other.uuids[0]) && self.uuids[1].eq(&other.uuids[1])
    }
}
impl<Prefix, T> Eq for PrefixedKey<Prefix, T> {}
impl<Prefix, T> PartialOrd for PrefixedKey<Prefix, T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        match self.uuids[0].partial_cmp(&other.uuids[0]) {
            Some(std::cmp::Ordering::Equal) => self.uuids[1].partial_cmp(&other.uuids[1]),
            x => x
        }
    }
}
impl<Prefix, T> Ord for PrefixedKey<Prefix, T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match self.uuids[0].cmp(&other.uuids[0]) {
            std::cmp::Ordering::Equal => self.uuids[1].cmp(&other.uuids[1]),
            x => x
        }
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

// TODO: document how keygen works for PrefixedKey

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
