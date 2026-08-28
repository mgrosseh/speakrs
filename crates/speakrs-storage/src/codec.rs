use std::{convert::Infallible, marker::PhantomData};

use bytemuck::{Pod, PodCastError};
use serde::{Deserialize, Serialize};
use sled::IVec;

use crate::tree::{TreeError, TreeResult};

pub trait DbValueCodec<T> {
    type EncodeError: std::error::Error + Send + Sync + 'static + Into<eyre::Error>;
    type DecodeError: std::error::Error + Send + Sync + 'static + Into<eyre::Error>;

    fn encode_ref(value: &T) -> Result<IVec, Self::EncodeError>;
    fn encode(value: T) -> Result<IVec, Self::EncodeError> {
        Self::encode_ref(&value)
    }
    fn decode_ref(ivec: &IVec) -> Result<T, Self::DecodeError>;
    fn decode(ivec: IVec) -> Result<T, Self::DecodeError> {
        Self::decode_ref(&ivec)
    }
}

/// Value encoding in JSON format using serde serialization.
pub struct SerdeJsonCodec;

impl<T> DbValueCodec<T> for SerdeJsonCodec
where
    T: Serialize + for<'a> Deserialize<'a>,
{
    type EncodeError = serde_json::Error;
    type DecodeError = serde_json::Error;
    fn encode_ref(value: &T) -> Result<IVec, Self::EncodeError> {
        Ok(serde_json::to_vec(value)?.into())
    }

    fn decode_ref(ivec: &IVec) -> Result<T, Self::EncodeError> {
        serde_json::from_slice(ivec)
    }
}

/// Direct "as bytes" encoding for plain old data types.
#[allow(unused)]
pub struct PodCodec;

impl<T: Pod> DbValueCodec<T> for PodCodec {
    type EncodeError = Infallible;
    type DecodeError = PodCastError;
    fn encode_ref(value: &T) -> Result<IVec, Self::EncodeError> {
        Ok(bytemuck::bytes_of(value).into())
    }

    fn decode_ref(ivec: &IVec) -> Result<T, Self::DecodeError> {
        bytemuck::try_pod_read_unaligned(ivec)
    }
}

/// A value of known type `T` that is currently in its [`IVec`] representation, but can be decoded using a known associated decoder.
pub struct EncodedValue<T, Codec> {
    pub raw: IVec,
    pub marker: PhantomData<(T, Codec)>,
}

impl<T, Codec> Clone for EncodedValue<T, Codec> {
    fn clone(&self) -> Self {
        Self {
            raw: self.raw.clone(),
            marker: self.marker,
        }
    }
}

pub trait Decodable {
    type Decoded;
    type DecodeError: std::error::Error + Send + Sync + 'static + Into<eyre::Error>;

    fn into_raw(self) -> IVec;

    /// Assume that given [`IVec`] represents this type of decodable value. Does not perform any runtime checks, the type validity must be enforced or checked externally. Failing to do that will result in
    /// decode errors or bogus decoded values.
    fn wrap(raw: IVec) -> Self
    where
        Self: Sized;

    /// A version of [`Decodable::wrap`] that maps over an optional. This shortcut is handy in the common case of nested mapping of [`Result<Option<IVec>>`] types.
    fn wrap_opt(raw: Option<IVec>) -> Option<Self>
    where
        Self: Sized,
    {
        raw.map(Self::wrap)
    }
    /// Decode the value into its proper type. Can be expensive, depending on the attached codec.
    /// By-value variant that can reuse the underlying [`IVec`].
    fn decode(self) -> Result<Self::Decoded, Self::DecodeError>;
}

impl Decodable for () {
    type Decoded = ();

    type DecodeError = Infallible;

    fn into_raw(self) -> IVec {
        IVec::default()
    }

    fn wrap(_raw: IVec) -> Self
    where
        Self: Sized,
    {
        ()
    }

    fn decode(self) -> Result<Self::Decoded, Self::DecodeError> {
        Ok(())
    }
}

impl<T, Codec> Decodable for EncodedValue<T, Codec>
where
    Codec: DbValueCodec<T>,
{
    type Decoded = T;
    type DecodeError = Codec::DecodeError;

    fn into_raw(self) -> IVec {
        self.raw
    }

    fn wrap(raw: IVec) -> Self
    where
        Self: Sized,
    {
        Self::from_raw(raw)
    }

    fn decode(self) -> Result<Self::Decoded, Self::DecodeError> {
        Codec::decode(self.raw)
    }
}

pub trait Encodable<Encoded> {
    type EncodeError: std::error::Error + Send + Sync + 'static + Into<eyre::Error>;
    fn encode(self) -> Result<Encoded, Self::EncodeError>;
}

impl<T, Codec> Encodable<EncodedValue<T, Codec>> for T
where
    Codec: DbValueCodec<T>,
{
    type EncodeError = Codec::EncodeError;

    fn encode(self) -> Result<EncodedValue<T, Codec>, Self::EncodeError> {
        Ok(EncodedValue::from_raw(Codec::encode(self)?))
    }
}

impl<T, Codec> Encodable<EncodedValue<T, Codec>> for &'_ T
where
    Codec: DbValueCodec<T>,
{
    type EncodeError = Codec::EncodeError;

    fn encode(self) -> Result<EncodedValue<T, Codec>, Self::EncodeError> {
        Ok(EncodedValue::from_raw(Codec::encode_ref(self)?))
    }
}

impl<T, Codec> EncodedValue<T, Codec> {
    fn from_raw(raw: IVec) -> Self {
        Self {
            raw,
            marker: PhantomData,
        }
    }

    pub fn decode(self) -> Result<T, Codec::DecodeError>
    where
        Codec: DbValueCodec<T>,
    {
        Decodable::decode(self)
    }
}

pub trait DecodeExt {
    type DecodeResult;
    fn decode(self) -> Self::DecodeResult;
}

impl<T> DecodeExt for T
where
    T: Decodable,
{
    type DecodeResult = Result<T::Decoded, T::DecodeError>;

    fn decode(self) -> Self::DecodeResult {
        Decodable::decode(self)
    }
}

impl<T> DecodeExt for Option<T>
where
    T: Decodable,
{
    type DecodeResult = Result<Option<T::Decoded>, T::DecodeError>;

    fn decode(self) -> Self::DecodeResult {
        self.map(Decodable::decode).transpose()
    }
}

impl<T> DecodeExt for Result<T, sled::Error>
where
    T: Decodable,
{
    type DecodeResult = TreeResult<T::Decoded>;

    fn decode(self) -> Self::DecodeResult {
        Ok(Decodable::decode(self?).map_err(TreeError::other)?)
    }
}
impl<T> DecodeExt for Result<Option<T>, sled::Error>
where
    T: Decodable,
{
    type DecodeResult = TreeResult<Option<T::Decoded>>;

    fn decode(self) -> Self::DecodeResult {
        self?
            .map(Decodable::decode)
            .transpose()
            .map_err(TreeError::other)
    }
}

pub trait IterDecodeExt: Iterator + Sized {
    type DecodeResult;

    fn decode(self) -> impl Iterator<Item = Self::DecodeResult>;
}

impl<T> IterDecodeExt for T
where
    T: Iterator,
    T::Item: DecodeExt,
{
    type DecodeResult = <T::Item as DecodeExt>::DecodeResult;

    fn decode(self) -> impl Iterator<Item = Self::DecodeResult> {
        self.map(DecodeExt::decode)
    }
}

// impl Iterator
// impl<T> T where T: Iterator {}
