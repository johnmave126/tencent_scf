# Rust Runtime for Serverless Compute Function on Tencent Cloud
[![crate][crate-image]][crate-link]
[![Docs][docs-image]][docs-link]
![Apache2/MIT licensed][license-image]

> :warning: **DISCLAIMER**: The author is **NOT** affiliated with Tencent and the project is **NOT** endorsed by Tencent. This project was developed solely out of personal use.

## Motivation
There is a well-known crate [lambda_runtime] that provides a runtime for rust as AWS Lambda. Recently I need to run some service on Tencent cloud and it is also well-known that Tencent Serverless Compute Function is simply a replica of AWS Lambda with a worse name. So I created this library with sligtly lighter dependencies than `lambda_runtime` and slightly different design decisions. It shouldn't be very hard to adapt an AWS Lambda to a Tencent Serverless Compute Function although concrete APIs are a little bit different.

## Example Function
The code below creates a simple function that receives an event with a `firstName` field and returns a message to the caller, adapted from `lambda_runtime`. Compiling the code requires `json` feature enabled.
```rust,no_run
use serde_json::{json, Value};
use tencent_scf::{convert::AsJson, make_scf, Context};

type Error = Box<dyn std::error::Error + Send + Sync + 'static>;

fn main() {
    let func = make_scf(func);
    tencent_scf::start(func);
}

fn func(event: Value, _: Context) -> Result<Value, Error> {
    let first_name = event["firstName"].as_str().unwrap_or("world");

    Ok(json!({ "message": format!("Hello, {}!", first_name) }))
}
```

## Deployment
The deployment is almost the same as [Deploy AWS Lambda]. User should try to follow that guide to create, compile and build the function. We outline the steps as follows:
1. Creating a Rust Function: follow the same instructions for AWS Lambda. The binary name should be `boostrap`, just like AWS Lambda.
2. Compiling and Building: `x86_64-unknown-linux-musl` target should be used, just like AWS Lambda.
3. Deploy the Function on Tencent Cloud: this is the step where things deviate a little bit:
    1. In the page for creating a serverless compute function,
    2. Choose "Custom Creation" for the "Creation Method".
    3. Choose "CustomRuntime" for the "Execution Environment".
    4. Choose "Upload zip archive" for the "Submission Method".
    5. Upload the `bootstrap.zip` file from step 2.
    6. (If needed) Set up other advanced configuration/triggers.
    7. Click "Finish".

## Roadmap
- Add a GitHub Workflow for auto testing.
- Add more examples.
- Add more tests.
- Add more built-in events and responses.

## License

Licensed under either of:

 * [Apache License, Version 2.0](http://www.apache.org/licenses/LICENSE-2.0)
 * [MIT license](http://opensource.org/licenses/MIT)

at your option.


[//]: # (badges and links)

[crate-image]: https://img.shields.io/crates/v/tencent_scf.svg
[crate-link]: https://crates.io/crates/tencent_scf
[docs-image]: https://docs.rs/tencent_scf/badge.svg
[docs-link]: https://docs.rs/tencent_scf/
[license-image]: https://img.shields.io/badge/license-Apache2.0/MIT-blue.svg

[lambda_runtime]: https://crates.io/crates/lambda_runtime
[Deploy AWS Lambda]: https://aws.amazon.com/blogs/opensource/rust-runtime-for-aws-lambda/
