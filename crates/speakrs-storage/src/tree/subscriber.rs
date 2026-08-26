use std::{task::Poll, time::Duration};

use pin_project_lite::pin_project;
use sled::{Event, IVec, Subscriber};

use crate::codec::Decodable;

use super::TypedTree;

/// An event that happened to a key that a subscriber is interested in.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum TypedEvent<K, Encoded> {
    /// A new complete (key, value) pair
    Insert {
        /// The key that has been set
        key: K,
        /// The value that has been set
        encoded: Encoded,
    },
    /// A deleted key
    Remove {
        /// The key that has been removed
        key: K,
    },
}

impl<K, Encoded> TypedEvent<K, Encoded> {
    /// Return the key associated with the `DBEvent`
    #[allow(unused)]
    pub fn key(&self) -> &K {
        match self {
            TypedEvent::Insert { key, .. } | TypedEvent::Remove { key } => key,
        }
    }

    pub fn wrap(event: Event) -> Self
    where
        K: From<IVec>,
        Encoded: Decodable,
    {
        match event {
            Event::Insert { key, value } => TypedEvent::Insert {
                key: K::from(key),
                encoded: Encoded::wrap(value),
            },
            Event::Remove { key } => TypedEvent::Remove { key: K::from(key) },
        }
    }
}

pin_project! {
    pub struct DBSubscriber<K, Encoded> {
        pub(super) tree: TypedTree<K, Encoded>,
        #[pin]
        pub(super) inner: Subscriber,
    }
}

#[allow(unused)]
#[derive(thiserror::Error, Debug)]
pub enum DBTimeoutError {
    #[error("Request expired: {0}")]
    RecvTimeoutError(#[from] std::sync::mpsc::RecvTimeoutError),
}

impl<K, Encoded> DBSubscriber<K, Encoded>
where
    K: From<IVec>,
    Encoded: Decodable,
{
    /// Attempts to wait for a value on this [`DBSubscriber`], returning
    /// an error if no event arrives within the provided `Duration`
    /// or if the backing `Db` shuts down.
    #[allow(unused)]
    pub fn next_timeout(
        &mut self,
        timeout: Duration,
    ) -> Result<TypedEvent<K, Encoded>, DBTimeoutError> {
        Ok(TypedEvent::wrap(self.inner.next_timeout(timeout)?))
    }
}

impl<K, Encoded> Future for DBSubscriber<K, Encoded>
where
    K: From<IVec>,
    Encoded: Decodable,
{
    type Output = Option<TypedEvent<K, Encoded>>;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        match self.project().inner.poll(cx) {
            Poll::Ready(opt) => Poll::Ready(opt.map(TypedEvent::wrap)),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<K, Encoded> Iterator for DBSubscriber<K, Encoded>
where
    K: From<IVec>,
    Encoded: Decodable,
{
    type Item = TypedEvent<K, Encoded>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(TypedEvent::wrap)
    }
}
