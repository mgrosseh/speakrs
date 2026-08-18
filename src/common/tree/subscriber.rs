use std::{task::Poll, time::Duration};
use anyhow::Result;

use pin_project_lite::pin_project;
use sled::{Event, IVec, Subscriber};

use crate::common::codec::DbValueCodec;

use super::DBTree;


/// An event that happened to a key that a subscriber is interested in.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum DBEvent<K, V> {
    /// A new complete (key, value) pair
    Insert {
        /// The key that has been set
        key: K,
        /// The value that has been set
        value: V,
    },
    /// A deleted key
    Remove {
        /// The key that has been removed
        key: K,
    },
}

impl<K, V> DBEvent<K, V> {
    /// Return the key associated with the `DBEvent`
    #[allow(unused)]
    pub fn key(&self) -> &K {
        match self {
            DBEvent::Insert { key, .. } | DBEvent::Remove { key } => key,
        }
    }
}

pin_project! {
    pub struct DBSubscriber<K, V, Codec, KeyGen> {
        pub(super) tree: DBTree<K, V, Codec, KeyGen>,
        #[pin]
        pub(super) inner: Subscriber,
    }
}

#[allow(unused)]
#[derive(thiserror::Error, Debug)]
pub enum DBTimeoutError {
    #[error("Error decoding from database: {0}")]
    DecodeError(#[from] anyhow::Error),
    #[error("Request expired: {0}")]
    RecvTimeoutError(#[from] std::sync::mpsc::RecvTimeoutError)
}

impl<K, V, Codec, KeyGen> DBSubscriber<K, V, Codec, KeyGen>
where
    K: AsRef<[u8]> + From<IVec>,
    Codec: DbValueCodec<V>, {
    /// Attempts to wait for a value on this [`DBSubscriber`], returning
    /// an error if no event arrives within the provided `Duration`
    /// or if the backing `Db` shuts down.
    #[allow(unused)]
    pub fn next_timeout(&mut self, timeout: Duration) -> std::result::Result<DBEvent<K, V>, DBTimeoutError> {
        Ok(Self::decode_event(self.inner.next_timeout(timeout)?)?)
    }

    fn decode_event(ev: Event) -> Result<DBEvent<K, V>> {
        Ok(match ev {
            Event::Insert { key, value } => DBEvent::Insert { key: K::from(key), value: Codec::decode_owned(value)? },
            Event::Remove { key } => DBEvent::Remove { key: K::from(key) },
        })
    }
}

impl<K, V, Codec, KeyGen> Future for DBSubscriber<K, V, Codec, KeyGen>
where
    K: AsRef<[u8]> + From<IVec>,
    Codec: DbValueCodec<V>, {
    type Output = Option<Result<DBEvent<K, V>>>;

    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Self::Output> {
        match self.project().inner.poll(cx) {
            Poll::Ready(opt) => Poll::Ready(opt.map(Self::decode_event)),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<K, V, Codec, KeyGen> Iterator for DBSubscriber<K, V, Codec, KeyGen>
where
    K: AsRef<[u8]> + From<IVec>,
    Codec: DbValueCodec<V>, {
    type Item = Result<DBEvent<K, V>>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(Self::decode_event)
    }
}
