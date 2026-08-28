pub mod compound;
pub mod generator;
pub mod integer;
pub mod singleton;
pub mod uuid;

pub use singleton::SingletonKey;
use sled::IVec;
pub use uuid::UuidKey;

pub trait Prefixed<P> {
    type Suffix;
    // Currently only used in tests
    #[allow(unused)]
    fn prefix(&self) -> P;
    fn suffix(&self) -> Self::Suffix;
}

pub trait DbKey: AsRef<[u8]> + From<IVec> + Into<IVec> {}
impl<T> DbKey for T where T: AsRef<[u8]> + From<IVec> + Into<IVec> {}

#[cfg(test)]
mod test {
    use super::*;
    use std::array;

    #[test]
    fn test_db_uuid_ordering() {
        let keys_in_order: [UuidKey<()>; 50] = array::from_fn(|_| UuidKey::new_now());

        let mut batch = sled::Batch::default();
        for key in &keys_in_order {
            batch.insert(*key, &[]);
        }

        let db = sled::Config::new().temporary(true).open().expect("open");
        db.apply_batch(batch).expect("Batch failed");

        assert_eq!(
            db.range(keys_in_order[3]..keys_in_order[7])
                .keys()
                .map(|r| r.map(UuidKey::<()>::from))
                .collect::<Result<Vec<_>, _>>()
                .as_ref(),
            Ok(&keys_in_order[3..7].to_vec())
        );

        assert_eq!(
            db.iter()
                .keys()
                .map(|r| r.map(UuidKey::<()>::from))
                .collect::<Result<Vec<_>, _>>(),
            Ok(keys_in_order.to_vec())
        );
    }
}
