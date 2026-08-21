use std::{collections::btree_map::IntoKeys, fmt::Display, marker::PhantomData};

use sled::IVec;

use crate::common::key::Prefixed;

pub type CompoundKey<Keys> = ConsKey<<Keys as IntoKeysList>::List>;

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ConsKey<Keys> {
    keys: Keys,
}

impl<PrefixKeys, Keys> Prefixed<ConsKey<PrefixKeys>> for ConsKey<Keys>
where
    Keys: HListSplit<PrefixKeys> + Clone,
{
    fn prefix(&self) -> ConsKey<PrefixKeys> {
        ConsKey {
            keys: self.keys.clone().split().0,
        }
    }
}

impl<Keys> ConsKey<Keys> {
    fn new(keys: impl IntoKeysList<List = Keys>) -> Self {
        Self {
            keys: keys.into_list(),
        }
    }
}

impl<Head> ConsKey<HCons<Head, HNil>> {
    fn of(head: Head) -> Self {
        Self {
            keys: HNil.cons(head),
        }
    }
}

impl<Keys> std::fmt::Display for ConsKey<Keys>
where
    Keys: Clone,
    for<'a, 'fmt> Keys: FoldableList<(), FoldDisplay<'a, 'fmt>, Output = std::fmt::Result>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "(")?;
        self.keys.clone().fold((), FoldDisplay(f))?;
        write!(f, ")")
    }
}

impl<Keys> std::fmt::Debug for ConsKey<Keys>
where
    Self: Display,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(self, f)
    }
}

struct FoldDisplay<'a, 'fmt>(&'a mut std::fmt::Formatter<'fmt>);

impl<'a, 'fmt, Head> HListFold<std::fmt::Result, Head> for FoldDisplay<'a, 'fmt>
where
    Head: std::fmt::Display,
{
    type Output = std::fmt::Result;

    fn fold_impl(&mut self, res: std::fmt::Result, head: Head) -> Self::Output {
        res?;
        write!(self.0, ", ")?;
        head.fmt(self.0)
    }
}

impl<'a, 'fmt, Head> HListFold<(), Head> for FoldDisplay<'a, 'fmt>
where
    Head: std::fmt::Display,
{
    type Output = std::fmt::Result;

    fn fold_impl(&mut self, res: (), head: Head) -> Self::Output {
        head.fmt(self.0)
    }
}

pub trait IntoKeysList {
    type List: HList;
    fn into_list(self) -> Self::List;
}

macro_rules! impl_into_keys {
    ($head:ident $($tail:ident)*) => {
        impl<$head, $($tail,)*> IntoKeysList for ($head, $($tail,)*) where ($($tail,)*): IntoKeysList {
            type List = HCons<$head, <($($tail,)*) as IntoKeysList>::List>;

            fn into_list(self) -> Self::List {
                #[allow(non_snake_case)]
                let ($head, $($tail,)*) = self;
                ($($tail,)*).into_list().cons($head)
            }
        }

        impl_into_keys!($($tail)*);
    };
    () => {};
}

impl IntoKeysList for () {
    type List = HNil;

    fn into_list(self) -> Self::List {
        HNil
    }
}

impl_into_keys!(A B C D E F G H I J);

pub trait HList
where
    Self: Sized,
{
    fn cons<Head>(self, head: Head) -> HCons<Head, Self> {
        HCons(head, self)
    }
}

trait HListFold<Start, Head> {
    type Output;
    fn fold_impl(&mut self, acc: Start, head: Head) -> Self::Output;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HNil;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HCons<Head, Tail>(Head, Tail);

impl HList for HNil {}
impl<Head, Tail> HList for HCons<Head, Tail> {}

trait HListSplit<Left> {
    type Right;
    fn split(self) -> (Left, Self::Right);
}

impl<Head, Tail> HListSplit<HNil> for HCons<Head, Tail> {
    type Right = Self;

    fn split(self) -> (HNil, Self::Right) {
        (HNil, self)
    }
}

impl<Head, Rest, Tail> HListSplit<HCons<Head, Rest>> for HCons<Head, Tail>
where
    Tail: HListSplit<Rest>,
    Rest: HList,
{
    type Right = Tail::Right;

    fn split(self) -> (HCons<Head, Rest>, Self::Right) {
        let (left, right) = self.1.split();
        (left.cons(self.0), right)
    }
}

trait FoldableList<Start, F> {
    type Output;
    fn fold(self, start: Start, f: F) -> Self::Output;
}

impl<Head, Tail, Start, F> FoldableList<Start, F> for HCons<Head, Tail>
where
    Tail: FoldableList<F::Output, F>,
    F: HListFold<Start, Head>,
{
    type Output = Tail::Output;
    fn fold(self, start: Start, mut f: F) -> Tail::Output {
        self.1.fold(f.fold_impl(start, self.0), f)
    }
}

impl<Start, F> FoldableList<Start, F> for HNil {
    type Output = Start;
    fn fold(self, start: Start, f: F) -> Start {
        start
    }
}

#[cfg(test)]
mod test {

    use crate::common::key::UuidKey;

    use super::*;

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", ConsKey::new((0, 1, 2))), "(0, 1, 2)");
        assert_eq!(format!("{}", ConsKey::new((2,))), "(2)");
        assert_eq!(
            format!("{}", ConsKey::new((5, 4, 3, 4, 5))),
            "(5, 4, 3, 4, 5)"
        );
    }

    type UnitKey = UuidKey<()>;

    #[test]
    fn test_prefix() {
        let head = UnitKey::new_now();
        let second = UnitKey::new_now();
        let key = ConsKey::new((head, second, UnitKey::new_now(), UnitKey::new_now()));

        assert_eq!(Prefixed::prefix(&key), ConsKey::of(head),);
        assert_eq!(Prefixed::prefix(&key), ConsKey::new((head, second)),);
    }
}
