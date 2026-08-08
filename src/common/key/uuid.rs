use serde::{Deserialize, Serialize};
use sled::{IVec, transaction::TransactionalTree};
use std::marker::PhantomData;
use uuid::{ClockSequence, ContextV7, Timestamp, Uuid};

use crate::common::key::generator::{GenerateKey, KeyGenerator};

#[derive(Serialize, Deserialize)]
#[serde(transparent)]
pub struct UuidKey<T> {
    pub(super) uid: Uuid,
    id_type: PhantomData<T>,
}

impl<T> std::fmt::Display for UuidKey<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.uid.fmt(f)
    }
}

impl<T> std::hash::Hash for UuidKey<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.uid.hash(state);
    }
}

impl<T> Ord for UuidKey<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.uid.cmp(&other.uid)
    }
}

impl<T> PartialOrd for UuidKey<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.uid.partial_cmp(&other.uid)
    }
}

impl<T> Eq for UuidKey<T> {}
impl<T> PartialEq for UuidKey<T> {
    fn eq(&self, other: &Self) -> bool {
        self.uid.eq(&other.uid)
    }
}

impl<T> Copy for UuidKey<T> {}
impl<T> Clone for UuidKey<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> std::fmt::Debug for UuidKey<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.uid.fmt(f)
    }
}

impl<T> From<IVec> for UuidKey<T> {
    fn from(value: IVec) -> Self {
        let bytes = value
            .as_array::<16>()
            .copied()
            .expect("Tried decoding a UUID of invalid byte length");
        Self::with_uid(Uuid::from_bytes(bytes))
    }
}

impl<T> UuidKey<T> {
    pub(super) fn with_uid(uid: Uuid) -> Self {
        Self {
            uid,
            id_type: PhantomData,
        }
    }

    pub fn new_now() -> Self {
        Self::with_uid(Uuid::now_v7())
    }
    pub fn new_at_time(ts: Timestamp) -> Self {
        Self::with_uid(Uuid::new_v7(ts))
    }
}

impl<T> AsRef<[u8]> for UuidKey<T> {
    fn as_ref(&self) -> &[u8] {
        self.uid.as_bytes()
    }
}

impl<T> From<UuidKey<T>> for IVec {
    fn from(value: UuidKey<T>) -> Self {
        IVec::from(value.uid.as_bytes())
    }
}

pub struct UuidNowKeygen<ClockSequence = ContextV7>(ClockSequence);

impl Default for UuidNowKeygen {
    fn default() -> Self {
        Self(ContextV7::new())
    }
}

impl<C: ClockSequence> KeyGenerator<C> for UuidNowKeygen<C> {
    fn construct(context: C, _: &TransactionalTree) -> Self {
        Self(context)
    }
}

impl<T> GenerateKey<UuidKey<T>> for UuidNowKeygen {
    fn generate_next(&self) -> UuidKey<T> {
        UuidKey::new_at_time(Timestamp::now(&self.0))
    }
}
