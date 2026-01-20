use std::pin::Pin;
use std::time::Duration;

use async_stream::stream;
use futures::Stream;
use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::{Method, Request, StatusCode};
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;

use crate::client::error::LlmError;

type HttpsClient =
    Client<hyper_rustls::HttpsConnector<hyper_util::client::legacy::connect::HttpConnector>, Full<Bytes>>;

/// Maximum number of retries for rate limit errors
const MAX_RETRIES: u32 = 5;

/// Base delay for exponential backoff (in milliseconds)
const BASE_DELAY_MS: u64 = 1000;

/// Maximum delay cap (in milliseconds)
const MAX_DELAY_MS: u64 = 60000;

#[derive(Clone)]
pub struct HttpClient {
    client: HttpsClient,
}

/// Calculate exponential backoff delay with jitter.
/// Also checks for Retry-After header in response body (Anthropic includes it in error message).
fn calculate_backoff_delay(attempt: u32, response_text: &str) -> Duration {
    // Try to extract retry-after from Anthropic's error response
    // Format: "... Please retry after X seconds."
    if let Some(seconds) = extract_retry_after(response_text) {
        return Duration::from_secs(seconds);
    }

    // Exponential backoff: base * 2^attempt + jitter
    let exponential_delay = BASE_DELAY_MS * (1 << attempt);
    let capped_delay = exponential_delay.min(MAX_DELAY_MS);

    // Add random jitter (0-25% of delay)
    let jitter = (capped_delay as f64 * 0.25 * rand_factor()) as u64;
    Duration::from_millis(capped_delay + jitter)
}

/// Extract retry-after seconds from Anthropic error message.
fn extract_retry_after(response_text: &str) -> Option<u64> {
    // Look for patterns like "retry after X seconds" or "retry_after": X
    let lower = response_text.to_lowercase();

    // Pattern: "retry after X seconds"
    if let Some(pos) = lower.find("retry after ") {
        let after_pos = pos + "retry after ".len();
        let remaining = &lower[after_pos..];
        if let Some(space_pos) = remaining.find(' ') {
            if let Ok(seconds) = remaining[..space_pos].trim().parse::<u64>() {
                return Some(seconds);
            }
        }
    }

    // Pattern: "retry_after": X (JSON field)
    if let Some(pos) = lower.find("\"retry_after\":") {
        let after_pos = pos + "\"retry_after\":".len();
        let remaining = &lower[after_pos..];
        // Skip whitespace
        let trimmed = remaining.trim_start();
        // Parse number
        let num_str: String = trimmed.chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(seconds) = num_str.parse::<u64>() {
            return Some(seconds);
        }
    }

    None
}

/// Simple pseudo-random factor between 0.0 and 1.0.
/// Uses current time nanoseconds for randomness (good enough for jitter).
fn rand_factor() -> f64 {
    use std::time::SystemTime;
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    (nanos % 1000) as f64 / 1000.0
}

impl HttpClient {
    pub fn new() -> Result<Self, LlmError> {
        let https = HttpsConnectorBuilder::new()
            .with_native_roots()
            .map_err(|e| {
                LlmError::new(
                    "TLS_INIT_FAILED",
                    format!("failed to load native TLS roots: {}", e),
                )
            })?
            .https_or_http()
            .enable_http1()
            .build();

        let client = Client::builder(TokioExecutor::new()).build(https);
        Ok(Self { client })
    }

    pub async fn get(&self, uri: &str) -> Result<String, LlmError> {
        let uri: hyper::Uri = uri
            .parse()
            .map_err(|e| LlmError::new("HTTP_INVALID_URI", format!("{}", e)))?;

        let request = Request::builder()
            .method(Method::GET)
            .uri(uri)
            .body(Full::new(Bytes::new()))
            .map_err(|e| LlmError::new("HTTP_REQUEST_BUILD", format!("{}", e)))?;

        let res = self
            .client
            .request(request)
            .await
            .map_err(|e| LlmError::new("HTTP_REQUEST_FAILED", format!("{}", e)))?;

        let body = res
            .collect()
            .await
            .map_err(|e| LlmError::new("HTTP_BODY_READ", format!("{}", e)))?
            .to_bytes();

        String::from_utf8(body.to_vec())
            .map_err(|e| LlmError::new("HTTP_INVALID_UTF8", format!("{}", e)))
    }

    pub async fn post(
        &self,
        uri: &str,
        headers: &[(&str, &str)],
        body: &str,
    ) -> Result<String, LlmError> {
        let parsed_uri: hyper::Uri = uri
            .parse()
            .map_err(|e| LlmError::new("HTTP_INVALID_URI", format!("{}", e)))?;

        let mut last_error = None;

        for attempt in 0..=MAX_RETRIES {
            let mut builder = Request::builder()
                .method(Method::POST)
                .uri(parsed_uri.clone());

            for (key, value) in headers {
                builder = builder.header(*key, *value);
            }

            let request = builder
                .body(Full::new(Bytes::from(body.to_string())))
                .map_err(|e| LlmError::new("HTTP_REQUEST_BUILD", format!("{}", e)))?;

            let res = self
                .client
                .request(request)
                .await
                .map_err(|e| LlmError::new("HTTP_REQUEST_FAILED", format!("{}", e)))?;

            let status = res.status();

            let response_body = res
                .collect()
                .await
                .map_err(|e| LlmError::new("HTTP_BODY_READ", format!("{}", e)))?
                .to_bytes();

            let response_text = String::from_utf8(response_body.to_vec())
                .map_err(|e| LlmError::new("HTTP_INVALID_UTF8", format!("{}", e)))?;

            // Check for rate limit (429) or overloaded (529)
            if status == StatusCode::TOO_MANY_REQUESTS || status.as_u16() == 529 {
                if attempt < MAX_RETRIES {
                    let delay = calculate_backoff_delay(attempt, &response_text);
                    tracing::warn!(
                        status = %status,
                        attempt = attempt + 1,
                        max_retries = MAX_RETRIES,
                        delay_ms = delay.as_millis(),
                        "Rate limited, retrying after delay"
                    );
                    tokio::time::sleep(delay).await;
                    last_error = Some(LlmError::new(
                        format!("HTTP_{}", status.as_u16()),
                        response_text,
                    ));
                    continue;
                }
            }

            // Return the response body (caller parses for API errors)
            return Ok(response_text);
        }

        // All retries exhausted
        Err(last_error.unwrap_or_else(|| {
            LlmError::new("RATE_LIMIT_EXHAUSTED", "Rate limit retries exhausted")
        }))
    }

    /// POST request that returns a stream of bytes for SSE handling.
    pub async fn post_stream(
        &self,
        uri: &str,
        headers: &[(&str, &str)],
        body: &str,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<Bytes, LlmError>> + Send>>, LlmError> {
        let parsed_uri: hyper::Uri = uri
            .parse()
            .map_err(|e| LlmError::new("HTTP_INVALID_URI", format!("{}", e)))?;

        let mut last_error = None;

        for attempt in 0..=MAX_RETRIES {
            let mut builder = Request::builder()
                .method(Method::POST)
                .uri(parsed_uri.clone());

            for (key, value) in headers {
                builder = builder.header(*key, *value);
            }

            let request = builder
                .body(Full::new(Bytes::from(body.to_string())))
                .map_err(|e| LlmError::new("HTTP_REQUEST_BUILD", format!("{}", e)))?;

            let res = self
                .client
                .request(request)
                .await
                .map_err(|e| LlmError::new("HTTP_REQUEST_FAILED", format!("{}", e)))?;

            let status = res.status();

            // Check for rate limit (429) or overloaded (529)
            if status == StatusCode::TOO_MANY_REQUESTS || status.as_u16() == 529 {
                let error_body = res
                    .collect()
                    .await
                    .map_err(|e| LlmError::new("HTTP_BODY_READ", format!("{}", e)))?
                    .to_bytes();
                let error_text = String::from_utf8_lossy(&error_body).to_string();

                if attempt < MAX_RETRIES {
                    let delay = calculate_backoff_delay(attempt, &error_text);
                    tracing::warn!(
                        status = %status,
                        attempt = attempt + 1,
                        max_retries = MAX_RETRIES,
                        delay_ms = delay.as_millis(),
                        "Rate limited on stream request, retrying after delay"
                    );
                    tokio::time::sleep(delay).await;
                    last_error = Some(LlmError::new(
                        format!("HTTP_{}", status.as_u16()),
                        error_text,
                    ));
                    continue;
                }

                // Max retries exceeded
                return Err(LlmError::new(
                    format!("HTTP_{}", status.as_u16()),
                    error_text,
                ));
            }

            // Check for other error status codes (no retry)
            if !status.is_success() {
                let error_body = res
                    .collect()
                    .await
                    .map_err(|e| LlmError::new("HTTP_BODY_READ", format!("{}", e)))?
                    .to_bytes();
                let error_text = String::from_utf8_lossy(&error_body);
                return Err(LlmError::new(
                    format!("HTTP_{}", status.as_u16()),
                    error_text.to_string(),
                ));
            }

            // Success - return the stream
            let response_body = res.into_body();
            let byte_stream = stream! {
                use http_body_util::BodyExt;
                let mut body = response_body;
                while let Some(frame_result) = body.frame().await {
                    match frame_result {
                        Ok(frame) => {
                            if let Some(data) = frame.data_ref() {
                                yield Ok(data.clone());
                            }
                        }
                        Err(e) => {
                            yield Err(LlmError::new("HTTP_STREAM_ERROR", format!("{}", e)));
                            break;
                        }
                    }
                }
            };

            return Ok(Box::pin(byte_stream)
                as Pin<Box<dyn Stream<Item = Result<Bytes, LlmError>> + Send>>);
        }

        // All retries exhausted
        Err(last_error.unwrap_or_else(|| {
            LlmError::new("RATE_LIMIT_EXHAUSTED", "Rate limit retries exhausted")
        }))
    }
}