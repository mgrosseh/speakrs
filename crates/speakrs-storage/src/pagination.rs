use std::ops::{Bound, Deref, RangeBounds};

use bytemuck::{Pod, Zeroable, bytes_of_mut};
use serde::{Deserialize, Serialize};

use crate::key::{
    UuidKey,
    compound::{ConsKey, IntoConsKey, KeyListSplit},
};

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum PaginationLimit {
    First(usize),
    Last(usize),
}

impl Default for PaginationLimit {
    fn default() -> Self {
        PaginationLimit::First(10)
    }
}

impl<Cursor> Default for Pagination<Cursor> {
    fn default() -> Self {
        Self {
            start: Bound::Unbounded,
            end: Bound::Unbounded,
            limit: Default::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Pagination<Cursor> {
    pub(crate) start: Bound<Cursor>,
    pub(crate) end: Bound<Cursor>,
    pub(crate) limit: PaginationLimit,
}

impl<Cursor> Pagination<Cursor> {
    pub fn add_prefix<PrefixKeys, JoinedKeys>(
        self,
        prefix: impl IntoConsKey<ConsKey<PrefixKeys>>,
    ) -> Pagination<ConsKey<JoinedKeys>>
    where
        JoinedKeys: KeyListSplit<PrefixKeys> + Clone,
        Cursor: IntoConsKey<ConsKey<JoinedKeys::Right>>,
        ConsKey<PrefixKeys>: Clone,
        JoinedKeys::Right: Pod,
    {
        let prefix = prefix.into_cons();

        let min_key: ConsKey<JoinedKeys::Right> = Zeroable::zeroed();
        let mut max_key: ConsKey<JoinedKeys::Right> = Zeroable::zeroed();
        bytes_of_mut(&mut max_key).fill(u8::MAX);

        Pagination {
            start: prefix_bound(self.start, prefix.clone(), min_key),
            end: prefix_bound(self.end, prefix, max_key),
            limit: self.limit,
        }
    }
}

fn prefix_bound<Cursor, PrefixKeys, JoinedKeys>(
    bound: Bound<Cursor>,
    prefix: ConsKey<PrefixKeys>,
    fallback: ConsKey<JoinedKeys::Right>,
) -> Bound<ConsKey<JoinedKeys>>
where
    JoinedKeys: KeyListSplit<PrefixKeys> + Clone,
    Cursor: IntoConsKey<ConsKey<JoinedKeys::Right>>,
    ConsKey<PrefixKeys>: Clone,
{
    match bound {
        Bound::Included(cursor) => Bound::Included(ConsKey::join(prefix, cursor)),
        Bound::Excluded(cursor) => Bound::Excluded(ConsKey::join(prefix, cursor)),
        Bound::Unbounded => Bound::Included(ConsKey::join(prefix, fallback)),
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Edge<Node, Cursor = UuidKey<Node>> {
    pub node: Node,
    pub cursor: Cursor,
}

impl<Node, Cursor> Edge<Node, Cursor> {}

impl<Node, Cursor> Deref for Edge<Node, Cursor> {
    type Target = Node;

    fn deref(&self) -> &Node {
        &self.node
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page<Node, Cursor = UuidKey<Node>> {
    pub edges: Vec<Edge<Node, Cursor>>,
    pub has_next_page: bool,
}

impl<'a, Node, Cursor> IntoIterator for &'a Page<Node, Cursor> {
    type Item = &'a Edge<Node, Cursor>;

    type IntoIter = std::slice::Iter<'a, Edge<Node, Cursor>>;

    fn into_iter(self) -> Self::IntoIter {
        self.edges.iter()
    }
}

impl<'a, Node, Cursor> IntoIterator for Page<Node, Cursor> {
    type Item = Edge<Node, Cursor>;

    type IntoIter = std::vec::IntoIter<Edge<Node, Cursor>>;

    fn into_iter(self) -> Self::IntoIter {
        self.edges.into_iter()
    }
}

impl<Node, Cursor> Page<Node, Cursor> {
    pub fn iter(&self) -> std::slice::Iter<'_, Edge<Node, Cursor>> {
        self.into_iter()
    }
    pub fn nodes(&self) -> impl Iterator<Item = &Node> + '_ {
        self.edges.iter().map(|e| &e.node)
    }
}

impl<Cursor> Pagination<Cursor> {
    pub fn limit(limit: PaginationLimit) -> Self {
        Self {
            limit,
            ..Default::default()
        }
    }

    pub fn map_cursor<T>(self, mut f: impl FnMut(Cursor) -> T) -> Pagination<T> {
        Pagination {
            start: self.start.map(&mut f),
            end: self.end.map(f),
            limit: self.limit,
        }
    }

    pub fn first(limit: usize) -> Self {
        Self::limit(PaginationLimit::First(limit))
    }

    pub fn last(limit: usize) -> Self {
        Self::limit(PaginationLimit::Last(limit))
    }

    pub fn range(self, range: impl RangeBounds<Cursor>) -> Self
    where
        Cursor: Clone, // Wouldn't be necessary if we used [`IntoBounds`] trait, but it's unstable right now.
    {
        Self {
            start: range.start_bound().cloned(),
            end: range.end_bound().cloned(),
            ..self
        }
    }

    pub fn opt_before(self, before: Option<Cursor>) -> Self {
        match before {
            Some(before) => self.before(before),
            None => self,
        }
    }

    pub fn opt_after(self, after: Option<Cursor>) -> Self {
        match after {
            Some(after) => self.after(after),
            None => self,
        }
    }

    pub fn before(self, before: Cursor) -> Self {
        Self {
            start: Bound::Excluded(before),
            ..self
        }
    }

    pub fn after(self, after: Cursor) -> Self {
        Self {
            end: Bound::Excluded(after),
            ..self
        }
    }
}
