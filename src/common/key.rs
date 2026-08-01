use serde::{Deserialize, Serialize};
use sled::IVec;
use std::marker::PhantomData;
use uuid::{Bytes, Uuid, timestamp::Timestamp};

#[derive(Serialize, Deserialize)]
#[serde(transparent)]
pub struct UuidKey<T> {
    uid: Uuid,
    id_type: PhantomData<*mut T>,
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
    fn with_uid(uid: Uuid) -> Self {
        Self {
            uid,
            id_type: PhantomData,
        }
    }

    fn from_bytes(bytes: uuid::Bytes) -> Self {
        Self::with_uid(Uuid::from_bytes(bytes))
    }

    pub fn new_now() -> Self {
        Self::with_uid(Uuid::now_v7())
    }
    pub fn new_at_time(ts: Timestamp) -> Self {
        Self::with_uid(Uuid::new_v7(ts))
    }

    pub fn with_prefix<P>(self, prefix: UuidKey<P>) -> PrefixedKey<UuidKey<P>, T> {
        PrefixedKey {
            uuids: [prefix.uid, self.uid],
            id_type: PhantomData,
        }
    }
}

impl<T> AsRef<[u8]> for UuidKey<T> {
    fn as_ref(&self) -> &[u8] {
        self.uid.as_bytes()
    }
}

pub struct PrefixedKey<Prefix, T> {
    uuids: [Uuid; 2],
    id_type: PhantomData<(Prefix, T)>,
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

#[derive(Clone, Copy)]
pub struct SingletonKey;

impl AsRef<[u8]> for SingletonKey {
    fn as_ref(&self) -> &[u8] {
        &[]
    }
}

impl From<IVec> for SingletonKey {
    fn from(_value: IVec) -> Self {
        Self
    }
}
