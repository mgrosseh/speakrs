use std::ops::Deref;

use crate::schema::DataStore;

pub struct Lens<'a, T, S> {
    pub focus: T,
    pub store: &'a DataStore<S>,
}

impl<'a, T, S> Lens<'a, T, S> {
    pub fn lens_ref(&self) -> Lens<'a, &'_ T, S> {
        Lens {
            focus: &self.focus,
            store: self.store,
        }
    }

    pub fn map_lens<U: LensWrap<S>>(
        self,
        f: impl FnOnce(T, &'a DataStore<S>) -> U,
    ) -> U::Wrapped<'a> {
        f(self.focus, self.store).to_lens(self.store)
    }
}

impl<T, S> Deref for Lens<'_, T, S> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.focus
    }
}

pub trait LensWrap<S> {
    type Wrapped<'db>
    where
        S: 'db;
    fn to_lens<'db>(self, store: &'db DataStore<S>) -> Self::Wrapped<'db>;
}

impl<'a, T, S> LensWrap<S> for Lens<'a, T, S> {
    type Wrapped<'db>
        = Self
    where
        S: 'db;

    fn to_lens<'db>(self, _store: &'db DataStore<S>) -> Self {
        self
    }
}

impl<T, E, S> LensWrap<S> for Result<T, E> {
    type Wrapped<'db>
        = Result<Lens<'db, T, S>, E>
    where
        S: 'db;

    fn to_lens<'db>(self, store: &'db DataStore<S>) -> Self::Wrapped<'db> {
        self.map(|focus| Lens { focus, store })
    }
}

impl<T, S> LensWrap<S> for Option<T> {
    type Wrapped<'db>
        = Option<Lens<'db, T, S>>
    where
        S: 'db;

    fn to_lens<'db>(self, store: &'db DataStore<S>) -> Self::Wrapped<'db> {
        self.map(|focus| Lens { focus, store })
    }
}
