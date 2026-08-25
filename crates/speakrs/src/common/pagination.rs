#![allow(unused)] // TODO

use std::ops::{Bound, RangeBounds};

use serde::{Deserialize, Serialize};

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
            before: Bound::Unbounded,
            after: Bound::Unbounded,
            limit: Default::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Pagination<Cursor> {
    before: Bound<Cursor>,
    after: Bound<Cursor>,
    limit: PaginationLimit,
}

impl<Cursor> Pagination<Cursor> {
    pub fn limit(limit: PaginationLimit) -> Self {
        Self {
            limit,
            ..Default::default()
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
        self.before(range.start_bound().cloned())
            .after(range.end_bound().cloned())
    }

    pub fn before(self, before: Bound<Cursor>) -> Self {
        Self { before, ..self }
    }

    pub fn after(self, after: Bound<Cursor>) -> Self {
        Self { after, ..self }
    }
}
