pub mod generator;
pub mod integer;
pub mod prefixed;
pub mod singleton;
pub mod uuid;

pub use prefixed::{PrefixedKey, PrefixedKeygen};
pub use singleton::SingletonKey;
use sled::IVec;
pub use uuid::{UuidKey, UuidNowKeygen};

// TODO: maybe rename; there could be confusion because of PrefixedKey and KeyPrefix
// KeyPrefix kind of
pub trait KeyPrefix<K> where K: AsRef<[u8]> + From<IVec> {
    /// Convert this KeyPrefix to a sled prefix
    fn to_prefix(self) -> IVec;
}

impl<T, X> KeyPrefix<PrefixedKey<UuidKey<T>, X>> for UuidKey<T> {
    fn to_prefix(self) -> IVec {
        IVec::from(self)
    }
}

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
