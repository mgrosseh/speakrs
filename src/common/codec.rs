use bytemuck::{Pod, PodCastError};
use serde::{Deserialize, Serialize};
use sled::IVec;

pub trait DbValueCodec<T> {
    // Separate owned variants to allow for specialized implementation that can benefit from access to T by value.
    // Default implementations just delegates to by-ref variants, so it is not required to implement it.

    type Error: std::error::Error + Send + Sync + 'static;
    fn encode(value: &T) -> Result<IVec, Self::Error>;
    fn encode_owned(value: T) -> Result<IVec, Self::Error> {
        Self::encode(&value)
    }
    fn decode(ivec: &IVec) -> Result<T, Self::Error>;
    fn decode_owned(ivec: IVec) -> Result<T, Self::Error> {
        Self::decode(&ivec)
    }
}

/// Value encoding in JSON format using serde serialization.
pub struct SerdeJsonCodec;

impl<T> DbValueCodec<T> for SerdeJsonCodec
where
    T: Serialize + for<'a> Deserialize<'a>,
{
    type Error = serde_json::Error;
    fn encode(value: &T) -> Result<IVec, Self::Error> {
        Ok(serde_json::to_vec(value)?.into())
    }

    fn decode(ivec: &IVec) -> Result<T, Self::Error> {
        serde_json::from_slice(ivec)
    }
}

/// Direct "as bytes" encoding for plain old data types.
#[allow(unused)]
pub struct PodCodec;

impl<T: Pod> DbValueCodec<T> for PodCodec {
    type Error = PodCastError;
    fn encode(value: &T) -> Result<IVec, Self::Error> {
        Ok(bytemuck::bytes_of(value).into())
    }

    fn decode(ivec: &IVec) -> Result<T, Self::Error> {
        bytemuck::try_pod_read_unaligned(ivec)
    }
}
