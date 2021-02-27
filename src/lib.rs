#![cfg_attr(docsrs, feature(doc_cfg))]

//! A rust runtime for Tencent Cloud Serverless Compute Function
//!
//! This library provides a basic custom runtime for a rust application
//! on Tencent Cloud as Serverless Compute Function. For complete setup,
//! see [Setup].
//!
//! # `start` vs `start_uncatched`
//! There are two varaints of `start` function to start the runtime, to sum up:
//! * [`start`] catches panic from the provided serverless compute function, but this requires the
//! function to be [`RefUnwindSafe`]. Most pure function should satisfy this.
//! * [`start_uncatched`] allows the serverless compute function to panic the whole process and relies on
//! the cloud to tear down the executor and restart. The downside of this is when a query causes
//! panic on the function, the runtime will be torn down. So the intialization has to be
//! performed after each panic. Sometimes this is necessary to reference `!RefUnwindSafe` types. Connection pool,
//! for example, usually is not unwind-safe.
//!
//! # Built-in Events
//! When enabling the correspondent `builtin-*` feature, auto serialization and deserialization of
//! certain built-in events and reponses are supported.
//!
//! Current supported built-in events/responses are:
//! * API Gateway: enabled by feature `builtin-api-gateway`, supports trigger as described in [API Gateway
//! Trigger]. `builtin::api::Request` and `builtin::api::Response` will be provided, which are
//! re-export of `http::Request` and `http::Response` with support of auto deserialization/serialization.
//!
//!   For more information, see [the module-level documentation][builtin::api].
//!   ```no_run
//!   # #[cfg(feature = "builtin-api-gateway")]
//!   # {
//!   use std::convert::Infallible;
//!  
//!   use tencent_scf::{
//!       builtin::api::{Request, Response, ResponseBuilder},
//!       make_scf, start, Context,
//!   };
//!  
//!   fn main() {
//!       let scf = make_scf(
//!           |event: Request<String>, _context: Context| -> Result<Response<String>, Infallible> {
//!               Ok(ResponseBuilder::new()
//!                   .status(200)
//!                   .body("Hello World".to_string())
//!                   .unwrap())
//!           },
//!       );
//!       start(scf);
//!   }
//!   # }
//!   ```
//!
//! # Custom Event and Response
//! Custom types of event and reponse are supported.
//!
//! ## Auto Serialization/Deserialization
//! Any type that implements [`serde::Deserialize`] and is marked as [`convert::AsJson`] can be
//! deserialized from JSON automatically when feature "json" is enabled. Similarly for types
//! implementing [`serde::Serialize`] and is marked as [`convert::AsJson`].
//!
//! ### Example
//! ```no_run
//! # #[cfg(feature = "json")]
//! # {
//! use std::convert::Infallible;
//!
//! use serde::{Deserialize, Serialize};
//! // with "json" feature enabled
//! use tencent_scf::{convert::AsJson, make_scf, start, Context};
//!
//! // define a custom event
//! #[derive(Deserialize)]
//! struct CustomEvent {
//!     a: i32,
//!     b: i32,
//! }
//!
//! // mark the custom event as json for auto deserialization
//! impl AsJson for CustomEvent {}
//!
//! // define a custom response
//! #[derive(Serialize)]
//! struct CustomResponse {
//!     a_plus_b: i32,
//! }
//!
//! // mark the custom response as json for auto serialization
//! impl AsJson for CustomResponse {}
//!
//! // make the scf
//! let scf = make_scf(
//!     |event: CustomEvent, _context: Context| -> Result<CustomResponse, Infallible> {
//!         Ok(CustomResponse {
//!             a_plus_b: event.a + event.b,
//!         })
//!     },
//! );
//! // start the runtime in the main function
//! start(scf);
//! # }
//! ```
//! ## Manual Serialization/Deserialization
//! User can also chose to implement [`convert::FromReader`] for incoming events and
//! [`convert::IntoBytes`] for outgoing response.
//!
//! [Setup]: https://github.com/johnmave126/tencent_scf/blob/master/README.md#deployment
//! [API Gateway Trigger]: https://cloud.tencent.com/document/product/583/12513
pub mod builtin;
pub mod convert;

mod helper;

use std::{
    env,
    fmt::Display,
    marker::PhantomData,
    panic::{self, RefUnwindSafe},
    str::FromStr,
};

use ureq::{Agent, AgentBuilder, Response};

/// The context of the invocation, assembled from environment variables and invocation headers.
///
/// The concrete description of each field can be found at [Custom Runtime API]
/// and [Built-in Environment Variables].
///
/// [Custom Runtime API]: https://cloud.tencent.com/document/product/583/47274#custom-runtime-.E8.BF.90.E8.A1.8C.E6.97.B6-api
/// [Built-in Environment Variables]: https://cloud.tencent.com/document/product/583/30228#.E5.B7.B2.E5.86.85.E7.BD.AE.E7.8E.AF.E5.A2.83.E5.8F.98.E9.87.8F

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Context {
    /// from header `memory_limit_in_mb`
    pub memory_limit_in_mb: usize,
    /// from header `time_limit_in_ms`
    pub time_limit_in_ms: usize,
    /// from header `request_id`
    pub request_id: String,
    /// from enviroment variable `SCF_NAMESPACE`
    pub namespace: String,
    /// from enviroment variables `SCF_FUNCTIONNAME`
    pub function_name: String,
    /// from enviroment variable `SCF_FUNCTIONVERSION`
    pub function_version: String,
    /// from enviroment variable `TENCENTCLOUD_REGION`
    pub region: String,
    /// from enviroment variable `TENCENTCLOUD_APPID`
    pub appid: String,
    /// from enviroment variable `TENCENTCLOUD_UIN`
    pub uin: String,
}

impl Context {
    /// Creates a context from values from header and from enviroment (via `env_context`).
    fn new(
        memory_limit_in_mb: usize,
        time_limit_in_ms: usize,
        request_id: String,
        env_context: &EnvContext,
    ) -> Self {
        Self {
            memory_limit_in_mb,
            time_limit_in_ms,
            request_id,
            namespace: env_context.namespace.clone(),
            function_name: env_context.function_name.clone(),
            function_version: env_context.function_version.clone(),
            region: env_context.region.clone(),
            appid: env_context.appid.clone(),
            uin: env_context.uin.clone(),
        }
    }
}

/// A subset of [`Context`] where the values are pulled from the environment variables
#[derive(Debug, Clone)]
struct EnvContext {
    namespace: String,
    function_name: String,
    function_version: String,
    region: String,
    appid: String,
    uin: String,
}

impl EnvContext {
    /// Load values from environment variables. The names of the variables are retrieved from [Built-in Environment Variables]
    ///
    /// # Panics
    /// The function will panic if any value is missing.
    ///
    /// [Built-in Environment Variables]: https://cloud.tencent.com/document/product/583/30228#.E5.B7.B2.E5.86.85.E7.BD.AE.E7.8E.AF.E5.A2.83.E5.8F.98.E9.87.8F
    fn load() -> Self {
        Self {
            namespace: env::var("SCF_NAMESPACE").unwrap(),
            function_name: env::var("SCF_FUNCTIONNAME").unwrap(),
            function_version: env::var("SCF_FUNCTIONVERSION").unwrap(),
            region: env::var("TENCENTCLOUD_REGION").unwrap(),
            appid: env::var("TENCENTCLOUD_APPID").unwrap(),
            uin: env::var("TENCENTCLOUD_UIN").unwrap(),
        }
    }
}

/// Main trait for a serverless compute function.
///
/// # Note
/// Depending on whether the concrete type is unwind-safe, user should choose between [`start`]
/// and [`start_uncatched`] to start the runtime. This trait **doesn't** imply `RefUnwindSafe`.
///
/// # Implement the Trait
/// Using a closure should cover most of the cases. If a struct/enum is used, the trait can be
/// implemented as:
/// ```no_run
/// use tencent_scf::{start, Context, ServerlessComputeFunction};
/// struct MyFunction {
///     some_attribute: String,
/// }
///
/// impl ServerlessComputeFunction for MyFunction {
///     // Type for input event
///     type Event = String;
///     // Type for output response
///     type Response = String;
///     // Type for possible error
///     type Error = std::convert::Infallible;
///
///     // Implement the execution of the function
///     fn call(
///         &self,
///         event: Self::Event,
///         context: Context,
///     ) -> Result<Self::Response, Self::Error> {
///         // We just concatenate the strings
///         Ok(event + &self.some_attribute)
///     }
/// }
///
/// start(MyFunction {
///     some_attribute: "suffix".to_string(),
/// });
/// ```
pub trait ServerlessComputeFunction {
    /// The type for incoming event.
    type Event;
    /// The type for outgoing response.
    type Response;
    /// The type for error(s) during the execution of the function.
    type Error;

    /// The actual execution of the function. Implementer is allowed to panic as it will be catched
    /// by the runtime.
    fn call(&self, event: Self::Event, context: Context) -> Result<Self::Response, Self::Error>;
}

/// A wrapper struct to convert a closure into a [`ServerlessComputeFunction`].
///
/// The main reason we need this is to make sure we can use associative type in
/// [`ServerlessComputeFunction`].
#[doc(hidden)]
pub struct Closure<Event, Response, Error, Function> {
    f: Function,
    phantom: PhantomData<panic::AssertUnwindSafe<(Event, Response, Error)>>,
}

#[doc(hidden)]
impl<Event, Response, Error, Function> ServerlessComputeFunction
    for Closure<Event, Response, Error, Function>
where
    Function: Fn(Event, Context) -> Result<Response, Error>,
{
    type Event = Event;
    type Response = Response;
    type Error = Error;
    fn call(&self, event: Event, context: Context) -> Result<Response, Error> {
        (&self.f)(event, context)
    }
}

/// Create a [`ServerlessComputeFunction`] from a closure.
///# Example
/// ```
/// # #[cfg(feature = "builtin-api-gateway")]
/// # {
/// use std::convert::Infallible;
///
/// use tencent_scf::{
///     builtin::api::{Request, Response, ResponseBuilder},
///     make_scf, Context,
/// };
///
/// let scf = make_scf(
///     |event: Request<String>, _context: Context| -> Result<Response<String>, Infallible> {
///         Ok(ResponseBuilder::new()
///             .status(200)
///             .body("Hello World".to_string())
///             .unwrap())
///     },
/// );
/// # }
/// ```
pub fn make_scf<Event, Response, Error, Function>(
    f: Function,
) -> Closure<Event, Response, Error, Function>
where
    Function: Fn(Event, Context) -> Result<Response, Error> + 'static,
{
    Closure {
        f,
        phantom: PhantomData,
    }
}

/// Start the runtime with the given serverless compute function.
///
/// Typically this should be the last call in the `main` function.
///
/// # Panic
/// The runtime will panic if any of the following happens:
/// * Fail to initialize, for example, expected environment vairable not found.
/// * Fail to communicate to upstream cloud.
/// * Receive malformed response from upstream cloud.
///
/// The runtime will **not** panic if the serverless compute function panics. Instead, the panic will
/// be captured and properly sent to the upstream cloud. The runtime may not be able to capture
/// **all** kinds of panic, see [`std::panic::catch_unwind`] for more information.
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
/// // define a custom event
/// #[derive(Deserialize)]
/// struct CustomEvent {
///     a: i32,
///     b: i32,
/// }
///
/// // mark the custom event as json for auto deserialization
/// impl AsJson for CustomEvent {}
///
/// // define a custom response
/// #[derive(Serialize)]
/// struct CustomResponse {
///     a_plus_b: i32,
/// }
///
/// // mark the custom response as json for auto serialization
/// impl AsJson for CustomResponse {}
///
/// fn main() {
///     // make the scf
///     let scf = make_scf(
///         |event: CustomEvent, _context: Context| -> Result<CustomResponse, Infallible> {
///             Ok(CustomResponse {
///                 a_plus_b: event.a + event.b,
///             })
///         },
///     );
///     // start the runtime in the main function
///     start(scf);
/// }
/// # }
/// ```
pub fn start<Event, Response, Error, ConvertEventError, ConvertResponseError, Function>(f: Function)
where
    Function: ServerlessComputeFunction<Event = Event, Response = Response, Error = Error>
        + RefUnwindSafe,
    Event: convert::FromReader<Error = ConvertEventError>,
    Response: convert::IntoBytes<Error = ConvertResponseError>,
    Error: Display,
    ConvertEventError: Display,
    ConvertResponseError: Display,
{
    // panic is fine, the function is still in consistent state, so we continue the runtime
    Runtime::new().run_with(f, |_, result| result.unwrap_or_else(Err));
}

/// Start the runtime with the given serverless compute function without catching panics.
///
/// Typically this should be the last call in the `main` function.
///
/// # Panic
/// The runtime will panic if any of the following happens:
/// * Fail to initialize, for example, expected environment vairable not found.
/// * Fail to communicate to upstream cloud.
/// * Receive malformed response from upstream cloud.
/// * The serverless compute function panics during execution.
///
/// # Example
/// Here is an example where the function must be started without panic catching:
/// ```no_run
/// # #[cfg(feature = "builtin-api-gateway")]
/// # {
/// use tencent_scf::{make_scf, start_uncatched, Context};
/// use ureq::AgentBuilder;
///
/// // build an http agent
/// // this object is not unwind-safe so any closure that captures it is not unwind-safe
/// let agent = AgentBuilder::new().build();
/// // make the scf
/// let scf = make_scf(
///     move |event: serde_json::Value,
///           _context: Context|
///           -> Result<serde_json::Value, ureq::Error> {
///         // do something using agent
///         let _resp = agent.get("http://example.com/").call()?;
///         Ok(event)
///     },
/// );
/// // start the runtime in the main function
/// start_uncatched(scf);
/// // this doesn't compile
/// // tencent_scf::start(scf);
/// # }
/// ```
pub fn start_uncatched<Event, Response, Error, ConvertEventError, ConvertResponseError, Function>(
    f: Function,
) where
    Function: ServerlessComputeFunction<Event = Event, Response = Response, Error = Error>,
    Event: convert::FromReader<Error = ConvertEventError>,
    Response: convert::IntoBytes<Error = ConvertResponseError>,
    Error: Display,
    ConvertEventError: Display,
    ConvertResponseError: Display,
{
    Runtime::new().run_with(f, |rt, result| {
        result.unwrap_or_else(|panic_message| {
            // The execution panicked, we can no longer continue
            rt.send_error_message(&panic_message);
            panic!(panic_message)
        })
    });
}

/// A struct that contains information for a runtime
struct Runtime {
    agent: Agent,
    ready_url: String,
    next_url: String,
    response_url: String,
    error_url: String,
    env_context: EnvContext,
}

impl Runtime {
    /// Create a runtime
    fn new() -> Self {
        // HTTP client pool
        let agent = AgentBuilder::new().build();

        // Extract cloud runtime server and port from environment variables
        let api_server = env::var("SCF_RUNTIME_API").unwrap();
        let api_port = env::var("SCF_RUNTIME_API_PORT").unwrap();

        // Assemble urls for upstream cloud communication
        // Gathered from [Custom Runtime
        // API](https://cloud.tencent.com/document/product/583/47274#custom-runtime-.E8.BF.90.E8.A1.8C.E6.97.B6-api)
        let ready_url = format!("http://{}:{}/runtime/init/ready", api_server, api_port);
        let next_url = format!("http://{}:{}/runtime/invocation/next", api_server, api_port);
        let response_url = format!(
            "http://{}:{}/runtime/invocation/response",
            api_server, api_port
        );
        let error_url = format!(
            "http://{}:{}/runtime/invocation/error",
            api_server, api_port
        );

        // Load context from environment variables
        let env_context = EnvContext::load();

        Self {
            agent,
            ready_url,
            next_url,
            response_url,
            error_url,
            env_context,
        }
    }

    /// Run the runtime
    ///
    /// A closure is passed on how to handle the result of an execution. The caller depends on this
    /// to handle panic differently
    fn run_with<
        Event,
        Response,
        Error,
        ConvertEventError,
        ConvertResponseError,
        Function,
        Handler,
    >(
        self,
        f: Function,
        result_handler: Handler,
    ) where
        Function: ServerlessComputeFunction<Event = Event, Response = Response, Error = Error>,
        Event: convert::FromReader<Error = ConvertEventError>,
        Response: convert::IntoBytes<Error = ConvertResponseError>,
        Error: Display,
        ConvertEventError: Display,
        ConvertResponseError: Display,
        Handler: Fn(&Runtime, Result<Result<Response, String>, String>) -> Result<Response, String>,
    {
        // Notify upstream cloud that we are ready
        self.notify_ready();

        // Main loop
        loop {
            // Fetch next event
            if let Some((event, context)) = self.next() {
                // The response is well-formed
                let result = result_handler(&self, Self::invoke(&f, event, context));
                // Send the result to the cloud
                self.send_result(result);
            }
        }
    }

    /// Notify the upstream cloud that the runtime is ready
    #[inline]
    fn notify_ready(&self) {
        // A space must be sent, otherwise the upstream cloud rejects the request.
        self.agent
            .post(&self.ready_url)
            .send_string(" ")
            .expect("fail to notify cloud about readiness");
    }

    /// Get the next event from upstream cloud
    #[inline]
    fn next<Event, ConvertError>(&self) -> Option<(Event, Context)>
    where
        Event: convert::FromReader<Error = ConvertError>,
        ConvertError: Display,
    {
        let resp = self
            .agent
            .get(&self.next_url)
            .call()
            .expect("fail to retrieve next event from cloud");

        match self.break_parts(resp) {
            Ok(parts) => Some(parts),
            Err(err) => {
                // Fail to parse the response
                self.send_error_message(&err);
                None
            }
        }
    }

    /// Invoke an scf.
    ///
    /// # Unwind Safety
    /// The function `f` is **not** required to be `RefUnwindSafe`. We maintain the variant that
    /// `f` is always in consistent state by exposing panic error to the caller. If the caller
    /// receives a `!RefUnwindSafe` function, it should panic the runtime if panic is detected to
    /// avoid `f` being used again.
    fn invoke<Event, Response, Error, ConvertEventError, ConvertResponseError, Function>(
        f: &Function,
        event: Event,
        context: Context,
    ) -> Result<Result<Response, String>, String>
    where
        Function: ServerlessComputeFunction<Event = Event, Response = Response, Error = Error>,
        Event: convert::FromReader<Error = ConvertEventError>,
        Response: convert::IntoBytes<Error = ConvertResponseError>,
        Error: Display,
        ConvertEventError: Display,
        ConvertResponseError: Display,
    {
        let invoke_result = {
            // Replace the panic handler to redirect panic messages.
            // This is a RAII construct so the panic handler should be reinstated after the
            // end of this block
            let panic_guard = helper::PanicGuard::new();
            let invoke_result = panic::catch_unwind({
                let scf = panic::AssertUnwindSafe(&f);
                // The event was deserialized from a byte stream and *should not* affect the
                // outer environment had a panic happens, but we cannot prevent crazy
                // implementater for `convert::FromReader` where they somehow introduce
                // `!UnwindSafe` types. Implementers are warned in the documentation for
                // `convert::FromReader` so for ergonomics we assert unwind-safety for
                // event
                let event = panic::AssertUnwindSafe(event);
                move || scf.0.call(event.0, context)
            })
            .map_err(|_| panic_guard.get_panic());
            invoke_result
        };
        invoke_result.map(|r| r.map_err(|err| format!("function failed with error: {}", err)))
    }

    /// Send the execution result to the upstream cloud
    fn send_result<Response, ConvertResponseError>(&self, result: Result<Response, String>)
    where
        Response: convert::IntoBytes<Error = ConvertResponseError>,
        ConvertResponseError: Display,
    {
        match result {
            Ok(response) => match response.into_bytes() {
                Ok(response) => {
                    // Send the result to the upstream
                    self.agent
                        .post(&self.response_url)
                        .send_bytes(&response)
                        .expect("fail to send response to the cloud");
                }
                Err(err) => {
                    // Fail to encode the response
                    self.send_error_message(&format!("fail to encode function response: {}", err));
                }
            },

            Err(err) => {
                self.send_error_message(&err);
            }
        }
    }

    /// Break a response for an event into payload and metadata, and try to parse the payload into
    /// event and collect metadata to form a context.
    #[doc(hidden)]
    fn break_parts<Event, ConvertError>(
        &self,
        invocation: Response,
    ) -> Result<(Event, Context), String>
    where
        Event: convert::FromReader<Error = ConvertError>,
        ConvertError: Display,
    {
        // Name of the headers from: [Custom Runtime
        // API](https://cloud.tencent.com/document/product/583/47274#custom-runtime-.E8.BF.90.E8.A1.8C.E6.97.B6-api)
        let memory_limit_in_mb = Self::parse_header(&invocation, "memory_limit_in_mb")?;
        let time_limit_in_ms = Self::parse_header(&invocation, "time_limit_in_ms")?;
        let request_id = Self::parse_header(&invocation, "request_id")?;

        let reader = invocation.into_reader();
        let event = Event::from_reader(reader)
            .map_err(|e| format!("failed to parse incoming invocation payload: {}", e))?;

        Ok((
            event,
            Context::new(
                memory_limit_in_mb,
                time_limit_in_ms,
                request_id,
                &self.env_context,
            ),
        ))
    }
    /// A helper function to parse a header in the response. Returns an error when the header doesn't
    /// exist or the parsing fails.
    #[inline]
    fn parse_header<T, Error>(response: &Response, header: &str) -> Result<T, String>
    where
        T: FromStr<Err = Error>,
        Error: Display,
    {
        match response.header(header) {
            Some(value) => value.parse().map_err(|e| {
                format!(
                    "fail to parse value of header {} of incoming invocation: {}",
                    header, e
                )
            }),
            None => Err(format!(
                "header {} is not present in the incoming invocation",
                header
            )),
        }
    }

    /// A helper function to send the error message to the upstream cloud
    #[inline]
    fn send_error_message(&self, message: &str) {
        self.agent
            .post(&self.error_url)
            .send_string(message)
            .expect("fail to send error message to the cloud");
    }
}
