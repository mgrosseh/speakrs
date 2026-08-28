pub mod iter;
pub mod subscriber;

use crate::{
    codec::Decodable,
    key::{DbKey, Prefixed},
    pagination::{Edge, Page, Pagination, PaginationLimit},
    tree::subscriber::DBSubscriber,
};

use iter::TypedTreeIter;
use sled::{
    IVec, Transactional, Tree,
    transaction::{
        ConflictableTransactionError, ConflictableTransactionResult, TransactionError,
        TransactionResult, TransactionalTree,
    },
};
use sled::{Result as SledResult, transaction::UnabortableTransactionError};
use std::{borrow::Borrow, fmt::Debug, marker::PhantomData, ops::RangeBounds};

/// Thin abstraction over [`sled::Tree`] with strongly typed key and value.
pub struct TypedTree<K, Encoded> {
    inner: Tree,
    marker: PhantomData<(K, Encoded)>,
}

impl<K, Encoded> Debug for TypedTree<K, Encoded> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TypedTree").finish()
    }
}

impl<K, Encoded> Clone for TypedTree<K, Encoded> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            marker: PhantomData,
        }
    }
}

impl<K, Encoded> TypedTree<K, Encoded> {
    pub fn open(db: &sled::Db, name: &str) -> sled::Result<Self> {
        Ok(Self::from_raw(db.open_tree(name)?))
    }

    pub(super) fn from_raw(inner: Tree) -> Self {
        Self {
            inner,
            marker: PhantomData,
        }
    }

    /// Returns the number of elements in this tree.
    ///
    /// Beware: performs a full O(n) scan under the hood.
    #[allow(unused)]
    fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns `true` if the `Tree` contains no elements.
    #[allow(unused)]
    fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TreeError {
    #[error(transparent)]
    Storage(#[from] sled::Error),
    #[error(transparent)]
    Other(#[from] eyre::Error),
}

impl TreeError {
    pub fn other(error: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Other(error.into())
    }
}

impl From<serde_json::Error> for TreeError {
    fn from(value: serde_json::Error) -> Self {
        TreeError::other(value)
    }
}

impl<E: std::error::Error + Send + Sync + 'static> From<TransactionError<E>> for TreeError {
    fn from(value: TransactionError<E>) -> Self {
        match value {
            TransactionError::Abort(error) => TreeError::Other(error.into()),
            TransactionError::Storage(error) => TreeError::Storage(error),
        }
    }
}

impl From<TreeError> for ConflictableTransactionError<TreeError> {
    fn from(value: TreeError) -> Self {
        match value {
            TreeError::Storage(error) => ConflictableTransactionError::Storage(error),
            TreeError::Other(error) => ConflictableTransactionError::Abort(error.into()),
        }
    }
}

pub type TreeResult<T> = Result<T, TreeError>;

impl<K, Encoded> TypedTree<K, Encoded>
where
    K: DbKey,
    Encoded: Decodable,
{
    fn wrap_entry((raw_key, raw_value): (IVec, IVec)) -> (K, Encoded) {
        (raw_key.into(), Encoded::wrap(raw_value))
    }

    fn wrap_opt_entry(entry: Option<(IVec, IVec)>) -> Option<(K, Encoded)> {
        entry.map(Self::wrap_entry)
    }

    fn wrap_res_entry<E>(entry: Result<(IVec, IVec), E>) -> Result<(K, Encoded), E> {
        entry.map(Self::wrap_entry)
    }

    /// Get a value corresponding to the key, or [`None`] if no value is present.
    pub fn get(&self, key: impl Borrow<K>) -> SledResult<Option<Encoded>> {
        self.inner.get(key.borrow()).map(Encoded::wrap_opt)
    }

    /// Inserts an already encoded key-value pair into the tree.
    ///
    /// If the tree did not have this key present, [`None`] is returned.
    ///
    /// If the tree did have this key present, the value is updated, and the old value is returned.
    pub fn insert(&self, key: K, value: Encoded) -> SledResult<Option<Encoded>> {
        self.inner
            .insert(key, value.into_raw())
            .map(Encoded::wrap_opt)
    }

    pub fn remove(&self, key: impl Borrow<K>) -> SledResult<Option<Encoded>> {
        self.inner.remove(key.borrow()).map(Encoded::wrap_opt)
    }
    #[allow(unused)] // TODO
    pub fn first(&self) -> SledResult<Option<(K, Encoded)>> {
        self.inner.first().map(Self::wrap_opt_entry)
    }
    /// Get the last key-value-pair in this tree.
    /// Keys are sorted by their bytes
    /// To retain the ordering of numerical types use big endian reprensentation
    pub fn last(&self) -> SledResult<Option<(K, Encoded)>> {
        self.inner.last().map(Self::wrap_opt_entry)
    }
    /// Get the next key-value-pair.
    /// That means, get K that using byte ordering is greater than [`key`], or none if [`key`] is the last key.
    /// Keys are sorted by their bytes.
    /// To retain the ordering of numerical types use big endian reprensentation.
    #[allow(unused)] // TODO
    pub fn get_gt(&self, key: impl Borrow<K>) -> SledResult<Option<(K, Encoded)>> {
        self.inner.get_gt(key.borrow()).map(Self::wrap_opt_entry)
    }

    /// Get the previous key-value-pair.
    /// That means, get K that using byte ordering is less than [`key`], or none if [`key`] is the first key.
    /// Keys are sorted by their bytes.
    /// To retain the ordering of numerical types use big endian reprensentation.
    #[allow(unused)] // TODO
    pub fn get_lt(&self, key: K) -> SledResult<Option<(K, Encoded)>> {
        self.inner.get_lt(key.borrow()).map(Self::wrap_opt_entry)
    }

    /// Iterate over the keys of this Tree
    #[allow(unused)]
    pub fn keys(self) -> impl DoubleEndedIterator<Item = sled::Result<K>> + Send + Sync {
        self.inner.iter().keys().map(|k| k.map(Into::into))
    }

    /// Iterate over the values of this Tree
    #[allow(unused)]
    pub fn values(self) -> impl DoubleEndedIterator<Item = sled::Result<Encoded>> + Send + Sync {
        self.inner.iter().values().map(|v| v.map(Encoded::wrap))
    }
}

pub struct TypedTransactionalTree<K, Encoded> {
    inner: TransactionalTree,
    marker: PhantomData<(K, Encoded)>,
}

impl<K, Encoded> Clone for TypedTransactionalTree<K, Encoded> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            marker: PhantomData,
        }
    }
}
impl<E, K, Encoded> Transactional<E> for TypedTree<K, Encoded> {
    type View = TypedTransactionalTree<K, Encoded>;

    fn make_overlay(&self) -> SledResult<sled::transaction::TransactionalTrees> {
        <Tree as Transactional<E>>::make_overlay(&self.inner)
    }

    fn view_overlay(overlay: &sled::transaction::TransactionalTrees) -> Self::View {
        TypedTransactionalTree::from_raw(<Tree as Transactional<E>>::view_overlay(overlay))
    }
}

impl<E, K, Encoded> Transactional<E> for &TypedTree<K, Encoded> {
    type View = TypedTransactionalTree<K, Encoded>;

    fn make_overlay(&self) -> SledResult<sled::transaction::TransactionalTrees> {
        <&Tree as Transactional<E>>::make_overlay(&&self.inner)
    }

    fn view_overlay(overlay: &sled::transaction::TransactionalTrees) -> Self::View {
        TypedTransactionalTree::from_raw(<&Tree as Transactional<E>>::view_overlay(overlay))
    }
}

/// A wrapper for performing transactions on multiple trees simultaneously.
///
/// Workaround needed to implementint [`sled::Transactional<E>`] on tuples of our own tree type.
/// Without this extra wrapper type, orphan rule prevents us from implementing `Transactional<E>` on `TypedTree` tuples without binding to `E` generic.
///
/// Usage:
///
/// ```ignore
/// Tx((&tree1, &tree2)).transaction(|(tx_tree1, tx_tree2|| {
///    // ...
/// })
/// ```
pub struct Tx<TypedTrees>(pub TypedTrees);

impl<TypedTrees> Tx<TypedTrees> {
    /// Runs a transaction, possibly retrying the passed-in closure if
    /// a concurrent conflict is detected that would cause a violation
    /// of serializability. This is the only trait method that
    /// you're most likely to use directly.

    // Explicitly declared to not rely on complicated trait resolution, allow for autocomplete to always work and make errors clearer.
    pub fn transaction<T, E>(
        &self,
        f: impl Fn(&<Self as Transactional<E>>::View) -> ConflictableTransactionResult<T, E>,
    ) -> TransactionResult<T, E>
    where
        Self: Transactional<E>,
    {
        Transactional::transaction(self, f)
    }
}

/// Helper trait that makes `impl_transactional_for_tx` macro significantly easier to implement.
/// Without this, the `Transactional<Err>` `impl` block would require separate instances of `K`, `V`, and `Codec` generics for each tuple element.
pub trait ToTransactional {
    type View;
    fn raw(&self) -> &Tree;
    fn wrap_view(view: TransactionalTree) -> Self::View;
}

impl<K, Encoded> ToTransactional for TypedTree<K, Encoded> {
    type View = TypedTransactionalTree<K, Encoded>;
    fn raw(&self) -> &Tree {
        &self.inner
    }
    fn wrap_view(view: TransactionalTree) -> Self::View {
        TypedTransactionalTree::from_raw(view)
    }
}

macro_rules! impl_transactional_for_tx {
    (@tree $_t:ident) => { &Tree };
    ($head:ident $($tail:ident)*) => {
        impl_transactional_for_tx!($($tail)*);

        #[allow(unused_parens, non_snake_case)]
        impl<Err, $head, $($tail,)*> Transactional<Err> for Tx<(&$head $(, &$tail)*)>
        where
            $head: ToTransactional,
            $($tail: ToTransactional,)*
        {
            type View = ($head::View  $(, $tail::View)*);
            fn make_overlay(&self) -> SledResult<sled::transaction::TransactionalTrees> {
                match self {
                    Tx(($head $(, $tail)*)) => {
                        let raw_trees = ($head.raw() $(, $tail.raw())*);
                        <(&Tree $(, impl_transactional_for_tx!(@tree $tail))*) as Transactional<Err>>::make_overlay(&raw_trees)
                    }
                }
            }

            fn view_overlay(overlay: &sled::transaction::TransactionalTrees) -> Self::View {
                let ($head $(, $tail)*) = <(&Tree $(, impl_transactional_for_tx!(@tree $tail))*) as Transactional<Err>>::view_overlay(overlay);
                ($head::wrap_view($head) $(, $tail::wrap_view($tail))*)
            }

        }
    };
    () => {};
}

// Implemented to the same tuple arity as `Transactional<Err> for (&Tree, ...)` in sled.
impl_transactional_for_tx!(A B C D E F G H I J K L M N);

impl<K, Encoded> TypedTransactionalTree<K, Encoded> {
    pub(super) fn from_raw(inner: TransactionalTree) -> Self {
        Self {
            inner,
            marker: PhantomData,
        }
    }
}

impl<K, Encoded> TypedTransactionalTree<K, Encoded>
where
    K: DbKey,
    Encoded: Decodable,
{
    pub fn get(&self, key: impl Borrow<K>) -> Result<Option<Encoded>, UnabortableTransactionError> {
        self.inner.get(key.borrow()).map(Encoded::wrap_opt)
    }

    pub fn insert(
        &self,
        key: K,
        value: Encoded,
    ) -> Result<Option<Encoded>, UnabortableTransactionError> {
        self.inner
            .insert(key, value.into_raw())
            .map(Encoded::wrap_opt)
    }

    pub fn remove(
        &self,
        key: impl Borrow<K>,
    ) -> Result<Option<Encoded>, UnabortableTransactionError> {
        self.inner
            .remove(key.borrow().as_ref())
            .map(Encoded::wrap_opt)
    }
}

impl<K, Encoded> TypedTree<K, Encoded>
where
    K: DbKey,
{
    /// Access a range of keys as an iterator.
    /// Keys are sorted by their bytes.
    /// To retain the ordering of numerical types use big endian reprensentation.
    pub fn range(&self, range: impl RangeBounds<K>) -> TypedTreeIter<K, Encoded> {
        TypedTreeIter {
            iter: self.inner.range(range),
            marker: PhantomData,
        }
    }

    pub fn page(&self, pagination: Pagination<K>) -> TreeResult<Page<Encoded::Decoded, K>>
    where
        Encoded: Decodable,
    {
        self.page_mapped(pagination, |cursor, encoded| {
            Ok(Edge {
                node: encoded.decode().map_err(TreeError::other)?,
                cursor,
            })
        })
    }

    pub fn page_mapped<T, K2>(
        &self,
        pagination: Pagination<K>,
        mut mapper: impl FnMut(K, Encoded) -> TreeResult<Edge<T, K2>>,
    ) -> TreeResult<Page<T, K2>>
    where
        Encoded: Decodable,
    {
        let mut range_iter = self.range((pagination.start, pagination.end));

        let apply_mapper = move |res: sled::Result<(K, Encoded)>| {
            let (k, encoded) = res?;
            mapper(k, encoded)
        };

        match pagination.limit {
            PaginationLimit::First(n) => {
                let edges = range_iter.by_ref().take(n).map(apply_mapper);
                Ok(Page {
                    edges: edges.collect::<Result<_, _>>()?,
                    has_next_page: range_iter.next().is_some(),
                })
            }
            PaginationLimit::Last(n) => {
                let mut reversed = range_iter.rev();
                let edges = reversed.by_ref().take(n).map(apply_mapper);
                Ok(Page {
                    edges: edges.collect::<Result<_, _>>()?,
                    has_next_page: reversed.next().is_some(),
                })
            }
        }
    }

    /// Create a double-ended iterator over the tuples of keys and
    /// values in this tree.
    pub fn iter(&self) -> TypedTreeIter<K, Encoded> {
        TypedTreeIter {
            iter: self.inner.iter(),
            marker: PhantomData,
        }
    }

    /// Subscribe to `DBEvent`s that happen to all keys.
    /// `DBEvents` for particular keys are guaranteed to be
    /// witnessed in the same order by all threads, but
    /// threads may witness different interleavings of
    /// `DBEvents` across different keys. If subscribers don't
    /// keep up with new writes, they will cause new writes
    /// to block. There is a buffer of 1024 items per
    /// `DBSubscriber`. This can be used to build reactive
    /// and replicated systems.
    ///
    /// `DBSubscriber` implements both `Iterator<Item = Result<DBEvent>>`
    /// and `Future<Output=Option<Event>>`
    #[allow(unused)]
    pub fn watch_all(&self) -> DBSubscriber<K, Encoded> {
        DBSubscriber {
            tree: self.clone(),
            inner: self.inner.watch_prefix(vec![]),
        }
    }

    // TODO: set merge opperation
    // TODO: pop_min, pop_max
    // TODO: batch translation layer
    // TODO: transaction translation layer
}

impl<K, Encoded> TypedTree<K, Encoded>
where
    K: AsRef<[u8]> + From<IVec>,
    // Codec: DbValueCodec<V>,
{
    // NOTE: below might pick up data not actually part of the intended prefix, since we type our prefix in a particular way.
    // If there ever are other key schemes, where one key might contain part of another without them being related (e.g. strings).
    // I've given this some thought and think its very unlikely to ever be a problem, but theoretically could.
    /// Subscribe to `DBEvent`s that happen to keys starting
    /// with `part`. `DBEvents` for particular keys are
    /// guaranteed to be witnessed in the same order by all
    /// threads, but threads may witness different interleavings
    /// of `DBEvents` across different keys. If subscribers don't
    /// keep up with new writes, they will cause new writes
    /// to block. There is a buffer of 1024 items per
    /// `DBSubscriber`. This can be used to build reactive
    /// and replicated systems.
    ///
    /// `DBSubscriber` implements both `Iterator<Item = Result<DBEvent>>`
    /// and `Future<Output=Option<Event>>`
    #[allow(unused)]
    pub fn watch_partial<P>(&self, part: P) -> DBSubscriber<K, Encoded>
    where
        K: Prefixed<P>,
        P: Into<IVec>,
    {
        DBSubscriber {
            tree: self.clone(),
            inner: self.inner.watch_prefix(part.into()),
        }
    }
}

#[cfg(test)]
mod test {
    use std::error::Error;

    use bytemuck::{Pod, Zeroable};
    use serde::{Deserialize, Serialize};
    use sled::Db;

    use crate::{
        codec::{DecodeExt, Encodable, EncodedValue, PodCodec},
        key::{
            UuidKey,
            compound::{CompoundKey, ConsKey},
            integer::IntegerKey,
        },
        table::SerdeTree,
    };

    use super::{subscriber::TypedEvent, *};

    fn mock_db() -> Db {
        sled::Config::new().temporary(true).open().expect("open")
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Zeroable, Pod, Serialize, Deserialize)]
    #[repr(C)]
    struct TestData {
        x: u32,
        y: [u8; 4],
    }

    #[derive(Debug, Clone, Copy, Zeroable, Pod)]
    #[repr(C)]
    struct Test2Data {
        foreign: TestKey,
        z: i32,
    }

    type TestKey = UuidKey<TestData>;
    type TestTree = TypedTree<TestKey, EncodedValue<TestData, PodCodec>>;

    type Test2Key = UuidKey<Test2Data>;
    type Test2Tree = TypedTree<Test2Key, EncodedValue<Test2Data, PodCodec>>;

    type RelationTree = TypedTree<CompoundKey<(TestKey, Test2Key)>, ()>;

    #[test]
    fn test_watch_all() -> eyre::Result<()> {
        let db = mock_db();
        let decl = SerdeTree::<TestData>::decl("test_watch_all");
        let tree = decl.open(&db)?;
        let subscriber = tree.watch_all();

        let thread = std::thread::spawn(move || -> eyre::Result<()> {
            let tree = decl.open(&db).expect("open");
            tree.insert(
                TestKey::new_now(),
                TestData {
                    x: 10,
                    y: [5, 1, 2, 7],
                }
                .encode()?,
            )?;
            Ok(())
        });

        for event in subscriber.take(1) {
            match event {
                TypedEvent::Insert { encoded, .. } => {
                    let value = encoded.decode()?;
                    assert_eq!(value.x, 10);
                    assert_eq!(value.y, [5, 1, 2, 7]);
                }
                TypedEvent::Remove { .. } => panic!("No remove should have been called!"),
            }
        }

        thread.join().unwrap()?;
        Ok(())
    }

    #[test]
    fn test_watch_partial() -> eyre::Result<()> {
        let db = mock_db();
        let test_tree = TestTree::open(&db, "test")?;
        let test2_tree = Test2Tree::open(&db, "test2")?;
        let relation_tree = RelationTree::open(&db, "relation")?;

        let test_key = TestKey::new_now();
        let test2_key = Test2Key::new_now();

        test_tree.insert(
            test_key,
            TestData {
                x: 1,
                y: [2, 3, 4, 5],
            }
            .encode()?,
        )?;

        let subscriber = relation_tree.watch_partial(ConsKey::of(test_key));

        let thread = std::thread::spawn(move || {
            test2_tree.insert(
                test2_key,
                Test2Data {
                    foreign: test_key,
                    z: 5,
                }
                .encode()?,
            )?;
            relation_tree.insert(ConsKey::new((test_key, test2_key)), ())?;
            Ok::<_, eyre::Error>(())
        });

        for event in subscriber.take(1) {
            match event {
                TypedEvent::Insert { encoded, key } => {
                    assert_eq!(encoded, ());
                    assert_eq!(key, ConsKey::new((test_key, test2_key)));
                    assert_eq!(
                        test_tree.get(test_key)?.map(|data| data.decode()),
                        Some(Ok(TestData {
                            x: 1,
                            y: [2, 3, 4, 5],
                        }))
                    )
                }
                TypedEvent::Remove { .. } => panic!("No remove should have been called!"),
            }
        }

        thread.join().unwrap()?;
        Ok(())
    }

    #[test]
    fn test_insert() -> Result<(), Box<dyn Error>> {
        let db = mock_db();
        let decl = TypedTree::<IntegerKey, EncodedValue<u64, PodCodec>>::decl("test_insert");
        let table = decl.open(&db).expect("open");

        table.insert(IntegerKey(2), 20.encode()?)?;

        assert_eq!(table.get(IntegerKey(2)).decode()?, Some(20));

        Ok(())
    }
}
