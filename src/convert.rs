//! Module that contains traits for converting incoming payload to `Event` and `Response` to
//! outgoing payload.
//!
//! # Overview
//! In order for the compiler to automatically deserialize an incoming event to some type, such
//! type must implement [`FromReader`]. Similarly a type must implement [`IntoBytes`] if one wishes
//! to serialize the type as a response.
//!
//! ## Example
//!
//! Here is an example of a webservice that capitalize all the letters of the input
//! ```no_run
//! use tencent_scf::{
//!     convert::{FromReader, IntoBytes},
//!     make_scf, start, Context,
//! };
//!
//! // Our custom event type
//! struct Event(String);
//!
//! // Simply read everything from the reader.
//! impl FromReader for Event {
//!     type Error = std::io::Error;
//!
//!     fn from_reader<Reader: std::io::Read + Send>(
//!         mut reader: Reader,
//!     ) -> Result<Self, Self::Error> {
//!         let mut buf = String::new();
//!         reader.read_to_string(&mut buf)?;
//!         Ok(Event(buf))
//!     }
//! }
//!
//! // Our custom response type
//! struct Response(String);
//!
//! // Simply turn a string into a byte array.
//! impl IntoBytes for Response {
//!     type Error = std::convert::Infallible;
//!
//!     fn into_bytes(self) -> Result<Vec<u8>, Self::Error> {
//!         Ok(String::into_bytes(self.0))
//!     }
//! }
//!
//! let scf = make_scf(
//!     |event: Event, _context: Context| -> Result<Response, std::convert::Infallible> {
//!         // capitalize the string
//!         Ok(Response(event.0.to_uppercase()))
//!     },
//! );
//! start(scf);
//! ```

use std::{convert::Infallible, io::Read};

#[cfg(feature = "json")]
pub use tencent_scf_derive::AsJson;

/// Marker trait for auto serialization/deserialization.
///
/// Many types may implement [`serde::Deserialize`] and/or [`serde::Serialize`] but it may not
/// always be the case that such type should be serialized/deserialized to/from JSON when runtime
/// receives a new event or to send a response. So user can have the runtime to do the
/// JSON conversion by marking the type as `AsJson`.
///
/// # Derivable
/// This trait can be used with `#[derive]`. When `derived`, no extra method is added, but the
/// compiler knows this type should be auto serialized/deserialized.
///
/// # Example
/// ```no_run
/// # #[cfg(feature = "json")]
/// # {
/// use std::convert::Infallible;
///
/// use serde::{Deserialize, Serialize};
/// use tencent_scf::{convert::AsJson, make_scf, start, Context};
///
/// #[derive(Deserialize, AsJson)]
/// struct MyEvent {
///     name: String,
///     width: usize,
///     height: usize,
/// }
///
/// #[derive(Serialize, AsJson)]
/// struct MyResponse {
///     area: usize,
/// }
///
/// let scf = make_scf(
///     |event: MyEvent, _context: Context| -> Result<MyResponse, Infallible> {
///         Ok(MyResponse {
///             area: event.width * event.height,
///         })
///     },
/// );
/// start(scf);
/// # }
/// ```
#[cfg(feature = "json")]
#[cfg_attr(docsrs, doc(cfg(feature = "json")))]
pub trait AsJson {}

/// [`serde_json::Value`] can be used as event and response.
///
/// # Example
/// ```no_run
/// # #[cfg(feature = "json")]
/// # {
/// use serde_json::{json, Value};
/// use tencent_scf::{convert::AsJson, make_scf, Context};
///
/// type Error = Box<dyn std::error::Error + Send + Sync + 'static>;
///
/// fn main() {
///     let func = make_scf(func);
///     tencent_scf::start(func);
/// }
///
/// fn func(event: Value, _: Context) -> Result<Value, Error> {
///     let first_name = event["firstName"].as_str().unwrap_or("world");
///
///     Ok(json!({ "message": format!("Hello, {}!", first_name) }))
/// }
/// # }
/// ```
#[cfg(feature = "json")]
#[cfg_attr(docsrs, doc(cfg(feature = "json")))]
impl AsJson for serde_json::Value {}

/// Trait for conversion from raw incoming invocation to event type
///
/// Custom event should implement this trait so that the runtime can deserialize the incoming
/// invocation into the desired event.
///
/// # Implement the Trait
/// One needs to provide a method that can consume a raw byte reader into the desired type.
/// ```
/// use tencent_scf::convert::FromReader;
///
/// // Our custom event type
/// struct Event(String);
///
/// // Simply read everything from the reader.
/// impl FromReader for Event {
///     type Error = std::io::Error;
///
///     fn from_reader<Reader: std::io::Read + Send>(
///         mut reader: Reader,
///     ) -> Result<Self, Self::Error> {
///         let mut buf = String::new();
///         reader.read_to_string(&mut buf)?;
///         Ok(Event(buf))
///     }
/// }
/// ```
pub trait FromReader: Sized {
    /// Possible error during the conversion. [`std::fmt::Display`] is all the runtime needs.
    type Error;
    /// Consume the `reader` and produce the deserialized output
    fn from_reader<Reader: Read + Send>(reader: Reader) -> Result<Self, Self::Error>;
}

/// Auto deserilization for types that are [`serde::Deserialize`] and marked as [`AsJson`].
#[cfg(feature = "json")]
#[cfg_attr(docsrs, doc(cfg(feature = "json")))]
impl<T> FromReader for T
where
    T: serde::de::DeserializeOwned + AsJson,
{
    type Error = serde_json::Error;
    fn from_reader<Reader: Read + Send>(reader: Reader) -> Result<Self, Self::Error> {
        serde_json::from_reader(reader)
    }
}

/// Auto deserialization into a byte array.
impl FromReader for Vec<u8> {
    type Error = std::io::Error;

    fn from_reader<Reader: Read + Send>(mut reader: Reader) -> Result<Self, Self::Error> {
        let mut buffer = Vec::new();
        reader.read_to_end(&mut buffer)?;
        Ok(buffer)
    }
}

/// Auto deserialization into a string.
impl FromReader for String {
    type Error = std::io::Error;

    fn from_reader<Reader: Read + Send>(mut reader: Reader) -> Result<Self, Self::Error> {
        let mut buffer = String::new();
        reader.read_to_string(&mut buffer)?;
        Ok(buffer)
    }
}

/// Auto deserialization into a unit, ignores any payload
impl FromReader for () {
    type Error = Infallible;

    fn from_reader<Reader: Read + Send>(_reader: Reader) -> Result<Self, Self::Error> {
        Ok(())
    }
}

/// Trait for converting response into raw bytes.
///
/// Custom response should implement this trait so that runtime can send raw bytes to the upstream
/// cloud.
///
/// # Implement the Trait
/// One needs to provide a method that can consume the object and produce a byte array.
/// ```
/// use tencent_scf::convert::IntoBytes;
///
/// // Our custom response type
/// struct Response(String);
///
/// // Simply turn a string into a byte array.
/// impl IntoBytes for Response {
///     type Error = std::convert::Infallible;
///
///     fn into_bytes(self) -> Result<Vec<u8>, Self::Error> {
///         Ok(String::into_bytes(self.0))
///     }
/// }
/// ```
pub trait IntoBytes {
    /// Possible error during the conversion. [`std::fmt::Display`] is all the runtime needs.
    type Error;
    /// Consume the object and serialize it into a byte array.
    fn into_bytes(self) -> Result<Vec<u8>, Self::Error>;
}

/// Auto serilization for types that are [`serde::Serialize`] and marked as [`AsJson`].
#[cfg(feature = "json")]
#[cfg_attr(docsrs, doc(cfg(feature = "json")))]
impl<T> IntoBytes for T
where
    T: serde::Serialize + AsJson,
{
    type Error = serde_json::Error;

    fn into_bytes(self) -> Result<Vec<u8>, Self::Error> {
        serde_json::to_vec(&self)
    }
}

/// Auto serialization for byte array (identity map).
impl IntoBytes for Vec<u8> {
    type Error = Infallible;

    fn into_bytes(self) -> Result<Vec<u8>, Self::Error> {
        Ok(self)
    }
}

/// Auto serilization for string.
impl IntoBytes for String {
    type Error = Infallible;

    fn into_bytes(self) -> Result<Vec<u8>, Self::Error> {
        Ok(String::into_bytes(self))
    }
}

/// Auto serilization for unit.
impl IntoBytes for () {
    type Error = Infallible;

    fn into_bytes(self) -> Result<Vec<u8>, Self::Error> {
        Ok(Vec::new())
    }
}
