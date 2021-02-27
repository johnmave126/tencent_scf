//! Module for auto serialization/deserialization of Web API Gateway Trigger.
//!
//! # Overview
//! This modules implements auto serialization/deserialization of Web API Gateway Trigger events
//! and responses as described in [API Gateway Trigger Overview].
//!
//! [`http::request::Request`], [`http::response::Response`] and [`http::response::Builder`] are
//! re-exported and user can directly use these types when constructing a serverless compute
//! function.
//!
//! # Note
//! There are some design decisions and limitation users may want to pay attention to:
//!
//! * Since the query string is already parsed by the upstream cloud, we found it
//! counter-productive to re-encode it and put it back into the uri of the request. So we leave the
//! path as-is as the uri of the request and use the `extension` of the request to hold the parsed
//! query string. User can acquire the `String`-keyed query string as follows:
//!   ```
//!   use tencent_scf::builtin::api::ext;
//!   # use http::request::Builder;
//!   # let req = Builder::new()
//!   #   .extension(ext::QueryString(Default::default()))
//!   #   .body::<Vec<u8>>(Default::default())
//!   #   .unwrap();
//!   // Assume `req` is the incoming request
//!   let query_string = &req.extensions().get::<ext::QueryString>().unwrap().0;
//!   ```
//! * It is allowed to set path parameters, header parameters and query string parameters in the
//! API dashboard. We wrap these parameters in [`ext::PathParameters`], [`ext::HeaderParameters`], and
//! [`ext::QueryStringParameters`] and put them in the extension of the request as well. User can
//! retrieve them like `ext::QueryString` above.
//! * The request context is wrapped in [`ext::RequestContext`] and can be retrieved just like
//! above.
//! * Multi-valued header is not supported in **request** due to a limitation in the
//! format used by the upstream cloud protocol.
//! * Currently we only support `Request<T>/Response<T>` where `T` is `String`, `Vec<u8>`, `()`, or
//! `serde::de::DeserializedOwn/serde::Serialize + AsJson`.
//! * For `Request<String>/Response<String>`, we assume the body is a utf-8 string and we don't do
//! any processing. For `Request<Vec<u8>>/Response<Vec<u8>>` however, we assume user wants a binary
//! format, so we will assume that the incoming request is base64 encoded (as required by the
//! cloud), and the runtime will try to decode the incoming event. We will also base64 encode the
//! response before sending it out as well (as required by the cloud). For
//! `Request<()>/Response<()>`, payload will be ignored.
//!
//! # Example
//! Here is an example of a very primitive reverse proxy:
//! ```no_run
//! # #[cfg(feature = "builtin-api-gateway")]
//! # {
//! use std::{io::Read, str::FromStr};
//!
//! use tencent_scf::{
//!     builtin::api::{
//!         ext,
//!         http::{
//!             header::{HeaderName, HeaderValue},
//!             Method,
//!         },
//!         Request, Response, ResponseBuilder,
//!     },
//!     make_scf, start, Context,
//! };
//! extern crate ureq;
//!
//! const FORWARD_TARGET: &str = "http://example.com";
//!
//! let scf = make_scf(
//!     |req: Request<Vec<u8>>, _context: Context| -> Result<Response<Vec<u8>>, ureq::Error> {
//!         // Check method
//!         if *req.method() == Method::GET {
//!             let forward_req = ureq::get(&format!("{}{}", FORWARD_TARGET, req.uri()));
//!             // Retrieve query string
//!             let query_string = &req.extensions().get::<ext::QueryString>().unwrap().0;
//!             let forward_req = query_string
//!                 .iter()
//!                 .fold(forward_req, |req, (key, value)| req.query(key, value));
//!             // Set headers
//!             let forward_req = req
//!                 .headers()
//!                 .iter()
//!                 .fold(forward_req, |req, (header, value)| {
//!                     req.set(header.as_str(), value.to_str().unwrap())
//!                 });
//!             // Send request
//!             let resp = forward_req.call()?;
//!             // Build response
//!             let mut forward_resp = ResponseBuilder::new().status(resp.status());
//!             let headers = forward_resp.headers_mut().unwrap();
//!             // Set headers
//!             for header in resp.headers_names() {
//!                 for value in resp.all(&header) {
//!                     headers.append(
//!                         HeaderName::from_str(&header).unwrap(),
//!                         HeaderValue::from_str(value).unwrap(),
//!                     );
//!                 }
//!             }
//!             // Read response
//!             let mut reader = resp.into_reader();
//!             let mut bytes: Vec<u8> = Vec::new();
//!             reader.read_to_end(&mut bytes)?;
//!             // Assemble response
//!             Ok(forward_resp.body(bytes).unwrap())
//!         } else {
//!             Ok(ResponseBuilder::new()
//!                 .status(501)
//!                 .body("Only GET method is supported".to_string().into_bytes())
//!                 .unwrap())
//!         }
//!     },
//! );
//! start(scf);
//!
//! # }
//! ```
//!
//! Here is another example using auto serialization/deserialization of types marked as [`AsJson`]
//! ```no_run
//! # #[cfg(feature = "builtin-api-gateway")]
//! # {
//! use std::convert::Infallible;
//!
//! use serde::{Deserialize, Serialize};
//! use tencent_scf::{
//!     builtin::api::{Request, Response, ResponseBuilder},
//!     convert::AsJson,
//!     make_scf, start, Context,
//! };
//!
//! #[derive(Deserialize, AsJson)]
//! struct MyEvent {
//!     name: String,
//!     width: usize,
//!     height: usize,
//! }
//!
//! #[derive(Serialize, AsJson)]
//! struct MyResponse {
//!     area: usize,
//! }
//!
//! let scf = make_scf(
//!     |req: Request<MyEvent>, _context: Context| -> Result<Response<MyResponse>, Infallible> {
//!         let event = req.body();
//!         Ok(ResponseBuilder::new()
//!             .status(200)
//!             .body(MyResponse {
//!                 area: event.width * event.height,
//!             })
//!             .unwrap())
//!     },
//! );
//! start(scf);
//! # }
//! ```
//!
//! [API Gateway Trigger Overview]: https://cloud.tencent.com/document/product/583/12513

use std::{
    collections::HashMap,
    convert::{TryFrom, TryInto},
    io::Read,
};

#[doc(no_inline)]
pub use http::{
    self,
    request::Request,
    response::{Builder as ResponseBuilder, Response},
};
use http::{
    header::{HeaderName, HeaderValue},
    request::Builder as RequestBuilder,
};
use itertools::Itertools;
use serde::{Deserialize, Serialize};

use crate::convert::{AsJson, FromReader, IntoBytes};

/// Used for any key-value dict
type Dict = HashMap<String, String>;

pub mod ext {
    //! Module that contains extensions in the `Request`.
    //!
    //! # Overview
    //! There are many additional information the upstream cloud sends whenever a request is made,
    //! which doesn't fit into a conventional `Request`. We leverage the extension API to include
    //! those information.
    //!
    //! It is guaranteed that all the types in this module will appear in the extension of a
    //! `Requset`. For example, to retrieve `PathParameters`, user can:
    //!   ```
    //!   use tencent_scf::builtin::api::ext;
    //!   # use http::request::Builder;
    //!   # let req = Builder::new()
    //!   #   .extension(ext::PathParameters(Default::default()))
    //!   #   .body::<Vec<u8>>(Default::default())
    //!   #   .unwrap();
    //!   // Assume `req` is the incoming request
    //!   let path_parameters = &req.extensions().get::<ext::PathParameters>().unwrap().0;
    //!   ```

    /// Additional request context returned by the upstream cloud.
    ///
    /// For more information, see [API Gateway Request Message Structure].
    ///
    /// To extract:
    /// ```
    /// # use tencent_scf::builtin::api::ext;
    /// # use http::request::Builder;
    /// # let req = Builder::new()
    /// #   .extension(ext::RequestContext::default())
    /// #   .body::<Vec<u8>>(Default::default())
    /// #   .unwrap();
    /// // Assume `req` is the incoming request
    /// let request_context = req.extensions().get::<ext::RequestContext>().unwrap();
    /// ```
    ///
    /// [API Gateway Request Message Structure]: https://cloud.tencent.com/document/product/583/12513#api-.E7.BD.91.E5.85.B3.E8.A7.A6.E5.8F.91.E5.99.A8.E7.9A.84.E9.9B.86.E6.88.90.E8.AF.B7.E6.B1.82.E4.BA.8B.E4.BB.B6.E6.B6.88.E6.81.AF.E7.BB.93.E6.9E.84
    #[derive(Default, Debug, Clone, super::Deserialize)]
    pub struct RequestContext {
        #[serde(rename = "serviceId")]
        pub service_id: String,
        pub path: String,
        #[serde(rename = "httpMethod")]
        pub method: String,
        // TODO: The actual payload doesn't contain `requestId`. File a bug.
        //#[serde(rename = "requestId")]
        //pub request_id: String,
        pub identity: super::Dict,
        #[serde(rename = "sourceIp")]
        pub source_ip: String,
        pub stage: String,
    }

    /// Path parameters defined in API Gateway Dashboard
    ///
    /// Wraps a key-value map.
    ///
    /// To extract:
    /// ```
    /// # use tencent_scf::builtin::api::ext;
    /// # use http::request::Builder;
    /// # let req = Builder::new()
    /// #   .extension(ext::PathParameters(Default::default()))
    /// #   .body::<Vec<u8>>(Default::default())
    /// #   .unwrap();
    /// // Assume `req` is the incoming request
    /// let path_parameters = &req.extensions().get::<ext::PathParameters>().unwrap().0;
    /// ```
    #[derive(Debug, Clone, PartialEq, Eq, super::Deserialize)]
    #[serde(transparent)]
    pub struct PathParameters(pub super::Dict);

    /// Parsed query string of the request
    ///
    /// Wraps a key-value map.
    ///
    /// To extract:
    /// ```
    /// # use tencent_scf::builtin::api::ext;
    /// # use http::request::Builder;
    /// # let req = Builder::new()
    /// #   .extension(ext::QueryString(Default::default()))
    /// #   .body::<Vec<u8>>(Default::default())
    /// #   .unwrap();
    /// // Assume `req` is the incoming request
    /// let query_string = &req.extensions().get::<ext::QueryString>().unwrap().0;
    /// ```
    #[derive(Debug, Clone, PartialEq, Eq, super::Deserialize)]
    #[serde(transparent)]
    pub struct QueryString(pub super::Dict);

    /// Query string parameters defined in API Gateway Dashboard
    ///
    /// Wraps a key-value map.
    ///
    /// To extract:
    /// ```
    /// # use tencent_scf::builtin::api::ext;
    /// # use http::request::Builder;
    /// # let req = Builder::new()
    /// #   .extension(ext::QueryStringParameters(Default::default()))
    /// #   .body::<Vec<u8>>(Default::default())
    /// #   .unwrap();
    /// // Assume `req` is the incoming request
    /// let query_string_parameters = &req
    ///     .extensions()
    ///     .get::<ext::QueryStringParameters>()
    ///     .unwrap()
    ///     .0;
    /// ```
    #[derive(Debug, Clone, PartialEq, Eq, super::Deserialize)]
    #[serde(transparent)]
    pub struct QueryStringParameters(pub super::Dict);

    /// Header parameters defined in API Gateway Dashboard
    ///
    /// Wraps a key-value map.
    ///
    /// To extract:
    /// ```
    /// # use tencent_scf::builtin::api::ext;
    /// # use http::request::Builder;
    /// # let req = Builder::new()
    /// #   .extension(ext::HeaderParameters(Default::default()))
    /// #   .body::<Vec<u8>>(Default::default())
    /// #   .unwrap();
    /// // Assume `req` is the incoming request
    /// let header_parameters = &req.extensions().get::<ext::HeaderParameters>().unwrap().0;
    /// ```
    #[derive(Debug, Clone, PartialEq, Eq, super::Deserialize)]
    #[serde(transparent)]
    pub struct HeaderParameters(pub super::Dict);
}

/// The raw event structure returned by the upstream cloud
///
/// Structure is defined in [API Gateway Request Message Structure].
///
/// [API Gateway Request Message Structure]: https://cloud.tencent.com/document/product/583/12513#api-.E7.BD.91.E5.85.B3.E8.A7.A6.E5.8F.91.E5.99.A8.E7.9A.84.E9.9B.86.E6.88.90.E8.AF.B7.E6.B1.82.E4.BA.8B.E4.BB.B6.E6.B6.88.E6.81.AF.E7.BB.93.E6.9E.84
#[doc(hidden)]
#[derive(Debug, Clone, Deserialize)]
pub struct Event {
    #[serde(rename = "requestContext")]
    context: ext::RequestContext,

    #[serde(rename = "httpMethod")]
    method: String,

    path: String,
    #[serde(rename = "pathParameters")]
    path_parameters: ext::PathParameters,

    #[serde(rename = "queryString")]
    query_string: ext::QueryString,
    #[serde(rename = "queryStringParameters")]
    query_string_parameters: ext::QueryStringParameters,

    headers: Dict,
    #[serde(rename = "headerParameters")]
    header_parameters: ext::HeaderParameters,

    body: Option<String>,
}

impl Event {
    /// Break down the event into a almost finished `RequestBuilder` and the body.
    fn into_request_builder(self) -> Result<(RequestBuilder, String), RequestParseError> {
        let mut req = RequestBuilder::new()
            .method(self.method.as_str())
            .uri(&self.path);
        // Set headers
        if let Some(headers) = req.headers_mut() {
            for (header, value) in &self.headers {
                let header = HeaderName::from_bytes(header.as_bytes())
                    .map_err(|_| RequestParseError::InvalidHeaderName(header.clone()))?;
                let value = HeaderValue::from_str(value).map_err(|_| {
                    RequestParseError::InvalidHeaderValue(header.as_str().to_string())
                })?;
                headers.append(header, value);
            }
        }

        if let Some(extensions) = req.extensions_mut() {
            // Populate extensions
            extensions.insert(self.context);
            extensions.insert(self.path_parameters);
            extensions.insert(self.query_string);
            extensions.insert(self.query_string_parameters);
            extensions.insert(self.header_parameters);
        }
        Ok((req, self.body.unwrap_or_default()))
    }
}

/// Container for a header value of the response
#[doc(hidden)]
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum WebResponseHeaderValue {
    Single(String),
    Multi(Vec<String>),
}

/// Raw structure for a response, to be sent to the server
///
/// Structure is defined in [API Gateway Response Structure].
///
/// [API Gateway Response Structure]: https://cloud.tencent.com/document/product/583/12513#api-.E7.BD.91.E5.85.B3.E8.A7.A6.E5.8F.91.E5.99.A8.E7.9A.84.E9.9B.86.E6.88.90.E5.93.8D.E5.BA.94.E8.BF.94.E5.9B.9E.E6.95.B0.E6.8D.AE.E7.BB.93.E6.9E.84
#[doc(hidden)]
#[derive(Debug, Clone, Serialize)]
pub struct WebResponse {
    #[serde(rename = "isBase64Encoded")]
    base64: bool,
    #[serde(rename = "statusCode")]
    status: u16,
    headers: HashMap<String, WebResponseHeaderValue>,
    body: String,
}

/// Possible errors when parsing an incoming request
#[doc(hidden)]
#[derive(Debug, thiserror::Error)]
pub enum RequestParseError {
    #[error("fail to deserialize event: {0}")]
    DeserializeError(#[from] serde_json::Error),
    #[error("fail to deserialize request body: {0}")]
    DeserializeBodyError(serde_json::Error),
    #[error("fail to assemble request struct: {0}")]
    AssembleError(#[from] http::Error),
    #[error("invalid header name for '{0}' in the request")]
    InvalidHeaderName(String),
    #[error("invalid header value for '{0}' in the request")]
    InvalidHeaderValue(String),
    #[error("fail to decode base64 body: {0}")]
    Base64DecodeError(#[from] base64::DecodeError),
}

#[doc(hidden)]
impl TryFrom<Event> for Request<String> {
    type Error = RequestParseError;

    fn try_from(value: Event) -> Result<Self, Self::Error> {
        let (req, body) = value.into_request_builder()?;
        Ok(req.body(body)?)
    }
}

#[doc(hidden)]
impl TryFrom<Event> for Request<Vec<u8>> {
    type Error = RequestParseError;

    fn try_from(value: Event) -> Result<Self, Self::Error> {
        let (req, body) = value.into_request_builder()?;
        let bytes = base64::decode(body.as_str())?;
        Ok(req.body(bytes)?)
    }
}

#[doc(hidden)]
impl TryFrom<Event> for Request<()> {
    type Error = RequestParseError;

    fn try_from(value: Event) -> Result<Self, Self::Error> {
        let (req, _body) = value.into_request_builder()?;
        Ok(req.body(())?)
    }
}

#[doc(hidden)]
impl<T> TryFrom<Event> for Request<T>
where
    T: serde::de::DeserializeOwned + AsJson,
{
    type Error = RequestParseError;

    fn try_from(value: Event) -> Result<Self, Self::Error> {
        let (req, body) = value.into_request_builder()?;
        let body = serde_json::from_str(&body).map_err(RequestParseError::DeserializeBodyError)?;
        Ok(req.body(body)?)
    }
}

/// Enable auto deserialization for `Request` types
///
/// The query part of the uri will be stored as [`ext::QueryString`] in the extension of the
/// request, leaving only the path in the `uri()`.
///
/// Currently only two types of payload is supported
/// * `Request<String>`: The body will be treated as a utf-8 string.
/// * `Request<Vec<u8>>`: The request body will be assumed as a base64-encoded string. The runtime
/// will decode it to `Vec<u8>`
/// * `Request<T>` where `T` is `DeserializeOwned` and marked as [`AsJson`]. The runtime will
/// deserialize the body automatically.
impl<T> FromReader for Request<T>
where
    Event: TryInto<Request<T>, Error = RequestParseError>,
{
    type Error = <Event as TryInto<Request<T>>>::Error;

    fn from_reader<Reader: Read + Send>(reader: Reader) -> Result<Self, Self::Error> {
        let event: Event = serde_json::from_reader(reader)?;
        event.try_into()
    }
}

/// Possible erros when encoding a outgoing response
#[doc(hidden)]
#[derive(Debug, thiserror::Error)]
pub enum ResponseEncodeError {
    #[error("fail to serialize response object: {0}")]
    SerializeError(#[from] serde_json::Error),
    #[error("fail to serialize response body: {0}")]
    SerializeBodyError(serde_json::Error),
    #[error("invalid header value for '{0}' in the response")]
    InvalidHeaderValue(String),
}

/// Break down an outgoing response into status code, map of headers, and body
fn break_response<T>(
    response: Response<T>,
) -> Result<(u16, HashMap<String, WebResponseHeaderValue>, T), ResponseEncodeError> {
    let (mut parts, body) = response.into_parts();
    let mut headers = HashMap::new();
    let mut header_iter = parts.headers.drain().peekable();
    while let Some((header, value)) = header_iter.next() {
        // It is guaranteed here that the header part is not None
        // see `http::header::HeaderMap::drain`
        let header = header.unwrap();
        if let Some(&(None, _)) = header_iter.peek() {
            // Consume all the following item with `None` header
            let values = std::iter::once(value)
                .chain(
                    header_iter
                        .peeking_take_while(|(header, _)| header.is_none())
                        .map(|(_, value)| value),
                )
                .map(|value| {
                    Ok(value
                        .to_str()
                        .map_err(|_| ResponseEncodeError::InvalidHeaderValue(header.to_string()))?
                        .to_string())
                })
                .collect::<Result<Vec<String>, ResponseEncodeError>>()?;
            headers.insert(header.to_string(), WebResponseHeaderValue::Multi(values));
        } else {
            // Single valued header
            let value = WebResponseHeaderValue::Single(
                value
                    .to_str()
                    .map_err(|_| ResponseEncodeError::InvalidHeaderValue(header.to_string()))?
                    .to_string(),
            );
            headers.insert(header.to_string(), value);
        }
    }

    Ok((parts.status.as_u16(), headers, body))
}

#[doc(hidden)]
impl TryFrom<Response<String>> for WebResponse {
    type Error = ResponseEncodeError;

    fn try_from(response: Response<String>) -> Result<Self, Self::Error> {
        let (status, headers, body) = break_response(response)?;
        Ok(Self {
            base64: false,
            status,
            headers,
            body,
        })
    }
}

#[doc(hidden)]
impl TryFrom<Response<Vec<u8>>> for WebResponse {
    type Error = ResponseEncodeError;

    fn try_from(response: Response<Vec<u8>>) -> Result<Self, Self::Error> {
        let (status, headers, body) = break_response(response)?;
        Ok(Self {
            base64: true,
            status,
            headers,
            body: base64::encode(&body),
        })
    }
}

#[doc(hidden)]
impl TryFrom<Response<()>> for WebResponse {
    type Error = ResponseEncodeError;

    fn try_from(response: Response<()>) -> Result<Self, Self::Error> {
        let (status, headers, _body) = break_response(response)?;
        Ok(Self {
            base64: false,
            status,
            headers,
            body: String::new(),
        })
    }
}

#[doc(hidden)]
impl<T> TryFrom<Response<T>> for WebResponse
where
    T: serde::Serialize + AsJson,
{
    type Error = ResponseEncodeError;

    fn try_from(response: Response<T>) -> Result<Self, Self::Error> {
        let (status, headers, body) = break_response(response)?;
        Ok(Self {
            base64: false,
            status,
            headers,
            body: serde_json::to_string(&body).map_err(ResponseEncodeError::SerializeBodyError)?,
        })
    }
}

/// Enable auto serialization for `Response` types
///
/// Currently only two types of payload is supported
/// * `Response<String>`: The body will be treated as a utf-8 string.
/// * `Response<Vec<u8>>`: The body will be assumed to be binary format. The runtime will perform a
/// base64 encoding to send it to the upstream cloud.
/// * `Response<T>` where `T` is `Serialize` and marked as [`AsJson`]. The runtime will
/// serialize the object to json automatically.
impl<T> IntoBytes for Response<T>
where
    Response<T>: TryInto<WebResponse, Error = ResponseEncodeError>,
{
    type Error = <Response<T> as TryInto<WebResponse>>::Error;

    fn into_bytes(self) -> Result<Vec<u8>, Self::Error> {
        let response: WebResponse = self.try_into()?;
        Ok(serde_json::to_vec(&response)?)
    }
}
