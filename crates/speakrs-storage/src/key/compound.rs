use std::fmt::Display;

use bytemuck::{AnyBitPattern, Pod, Zeroable};
use sled::IVec;

use crate::key::Prefixed;

pub type CompoundKey<Keys> = ConsKey<<Keys as IntoKeyList>::List>;

#[derive(Clone, Copy, PartialEq, Eq, Zeroable, Pod)]
#[repr(transparent)]
pub struct ConsKey<Keys> {
    keys: Keys,
}

impl<PrefixKeys, Keys> Prefixed<ConsKey<PrefixKeys>> for ConsKey<Keys>
where
    Keys: KeyListSplit<PrefixKeys> + Clone,
{
    fn prefix(&self) -> ConsKey<PrefixKeys> {
        ConsKey {
            keys: self.keys.clone().split().0,
        }
    }
}

impl<Keys> ConsKey<Keys> {
    pub fn new(keys: impl IntoKeyList<List = Keys>) -> Self {
        Self {
            keys: keys.into_list(),
        }
    }
}

impl<Head> ConsKey<KCons<Head, KNil>> {
    pub fn of(head: Head) -> Self {
        Self {
            keys: KNil.cons(head),
        }
    }
}

impl<Keys> AsRef<[u8]> for ConsKey<Keys>
where
    Keys: Pod,
{
    fn as_ref(&self) -> &[u8] {
        bytemuck::bytes_of(self)
    }
}

impl<Keys> From<ConsKey<Keys>> for IVec
where
    Keys: Pod,
{
    fn from(value: ConsKey<Keys>) -> Self {
        value.as_ref().into()
    }
}

impl<Keys> From<IVec> for ConsKey<Keys>
where
    Self: AnyBitPattern,
{
    fn from(value: IVec) -> Self {
        bytemuck::pod_read_unaligned(value.as_ref())
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

impl<'a, 'fmt, Head> KeyListFold<std::fmt::Result, Head> for FoldDisplay<'a, 'fmt>
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

impl<'a, 'fmt, Head> KeyListFold<(), Head> for FoldDisplay<'a, 'fmt>
where
    Head: std::fmt::Display,
{
    type Output = std::fmt::Result;

    fn fold_impl(&mut self, _res: (), head: Head) -> Self::Output {
        head.fmt(self.0)
    }
}

pub trait IntoKeyList {
    type List: KeyList;
    fn into_list(self) -> Self::List;
}

macro_rules! impl_into_keys {
    ($head:ident $($tail:ident)*) => {
        impl<$head, $($tail,)*> IntoKeyList for ($head, $($tail,)*) where ($($tail,)*): IntoKeyList {
            type List = KCons<$head, <($($tail,)*) as IntoKeyList>::List>;

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

impl IntoKeyList for () {
    type List = KNil;

    fn into_list(self) -> Self::List {
        KNil
    }
}

impl_into_keys!(A B C D E F G H I J);

pub trait KeyList
where
    Self: Sized,
{
    fn cons<Head>(self, head: Head) -> KCons<Head, Self> {
        KCons(head, self)
    }
}

trait KeyListFold<Start, Head> {
    type Output;
    fn fold_impl(&mut self, acc: Start, head: Head) -> Self::Output;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Zeroable, Pod)]
#[repr(transparent)]
pub struct KNil;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Zeroable, Pod)]
#[repr(C, packed)]
pub struct KCons<Head, Tail>(Head, Tail);

impl KeyList for KNil {}
impl<Head, Tail> KeyList for KCons<Head, Tail> {}

trait KeyListSplit<Left> {
    type Right;
    fn split(self) -> (Left, Self::Right);
}

impl<Head, Tail> KeyListSplit<KNil> for KCons<Head, Tail> {
    type Right = Self;

    fn split(self) -> (KNil, Self::Right) {
        (KNil, self)
    }
}

impl<Head, Rest, Tail> KeyListSplit<KCons<Head, Rest>> for KCons<Head, Tail>
where
    Tail: KeyListSplit<Rest>,
    Rest: KeyList,
{
    type Right = Tail::Right;

    fn split(self) -> (KCons<Head, Rest>, Self::Right) {
        let (left, right) = self.1.split();
        (left.cons(self.0), right)
    }
}

trait FoldableList<Start, F> {
    type Output;
    fn fold(self, start: Start, f: F) -> Self::Output;
}

impl<Head, Tail, Start, F> FoldableList<Start, F> for KCons<Head, Tail>
where
    Tail: FoldableList<F::Output, F>,
    F: KeyListFold<Start, Head>,
{
    type Output = Tail::Output;
    fn fold(self, start: Start, mut f: F) -> Tail::Output {
        self.1.fold(f.fold_impl(start, self.0), f)
    }
}

impl<Start, F> FoldableList<Start, F> for KNil {
    type Output = Start;
    fn fold(self, start: Start, _f: F) -> Start {
        start
    }
}

#[cfg(test)]
mod test {

    use crate::key::UuidKey;

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
