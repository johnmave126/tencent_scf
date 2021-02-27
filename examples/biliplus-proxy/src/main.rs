use std::{io::Read, str::FromStr, time::Duration};

use tencent_scf::{
    builtin::api::{
        ext,
        http::header::{self, HeaderName, HeaderValue},
        Request, Response, ResponseBuilder,
    },
    make_scf, Context,
};
use ureq::AgentBuilder;

const HOST: &str = "api.bilibili.com";
const PC_URL: &str = "https://api.bilibili.com/pgc/player/web/playurl";
const APP_URL: &str = "https://api.bilibili.com/pgc/player/web/playurl";
const TIMEOUT: u64 = 5;

// Custom error type
#[derive(Debug, thiserror::Error)]
enum Error {
    #[error("request error: {0}")]
    RequestError(#[from] ureq::Error),
    #[error("i/o error: {0}")]
    IoError(#[from] std::io::Error),
}

fn main() {
    // Set up connection pool
    let agent = AgentBuilder::new()
        .timeout(Duration::from_secs(TIMEOUT))
        .max_idle_connections(1)
        .build();

    let scf = make_scf(
        move |req: Request<String>, _: Context| -> Result<Response<Vec<u8>>, Error> {
            // drop the body since we don't need it
            let (mut parts, _) = req.into_parts();
            let query_string = parts.extensions.remove::<ext::QueryString>().unwrap().0;
            let forward_req = if query_string.get("platform").map(String::as_str) == Some("android")
            {
                parts.headers.remove(header::USER_AGENT);
                agent
                    .get(APP_URL)
                    .set("User-Agent", "Bilibili Freedoooooom/MarkII")
            } else {
                agent.get(PC_URL)
            };

            // Clean up headers
            parts.headers.remove(header::HOST);
            parts.headers.remove(header::REFERER);

            // Set headers
            let forward_req = parts
                .headers
                .iter()
                .filter_map(|(header, value)| {
                    value.to_str().ok().map(|value| (header.as_str(), value))
                })
                .fold(forward_req, |req, (header, value)| req.set(header, value));

            // Set query
            let forward_req = query_string
                .iter()
                .fold(forward_req, |req, (key, value)| req.query(key, value));

            // Send request
            let resp = forward_req.set("Host", HOST).call()?;
            // Build response
            let forward_resp = ResponseBuilder::new().status(resp.status());
            // Set headers
            let forward_resp = resp
                .headers_names()
                .into_iter()
                .filter(|header| !matches!(&header[..], "connection" | "transfer-encoding"))
                .fold(forward_resp, |forward_resp, header| {
                    // Try to convert header to a HeaderName
                    if let Ok(header_name) = HeaderName::from_str(&header) {
                        resp.all(&header)
                            .into_iter()
                            .fold(forward_resp, |forward_resp, value| {
                                // Try to convert value to a HeaderValue
                                if let Ok(header_value) = HeaderValue::from_str(value) {
                                    forward_resp.header(&header_name, header_value)
                                } else {
                                    forward_resp
                                }
                            })
                    } else {
                        forward_resp
                    }
                });
            // Read the raw body. We expect server to send compressed data so we don't treat it as
            // `String`
            let mut body = Vec::new();
            resp.into_reader().read_to_end(&mut body)?;
            Ok(forward_resp.body(body).unwrap())
        },
    );

    // Capturing `agent` is not unwind-safe, so we start thr runtime in uncatched mode
    tencent_scf::start_uncatched(scf);
}
