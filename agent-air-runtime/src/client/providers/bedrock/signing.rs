//! AWS SigV4 signing implementation for Bedrock API requests.

use super::BedrockCredentials;
use crate::client::error::LlmError;

use ring::digest;
use ring::hmac;

// =============================================================================
// Constants
// =============================================================================

/// AWS Bedrock service name for signing.
const SERVICE_NAME: &str = "bedrock";

/// SHA256 algorithm identifier for AWS SigV4.
const AWS_ALGORITHM: &str = "AWS4-HMAC-SHA256";

/// Signing key prefix.
const AWS4_PREFIX: &str = "AWS4";

/// Request type suffix.
const AWS4_REQUEST: &str = "aws4_request";

/// Content type for JSON.
const CONTENT_TYPE_JSON: &str = "application/json";

/// Content type for event stream (streaming responses).
const CONTENT_TYPE_EVENT_STREAM: &str = "application/vnd.amazon.eventstream";

// =============================================================================
// Public API
// =============================================================================

/// Sign an HTTP request using AWS SigV4.
///
/// Returns a vector of header name/value pairs to include in the request.
pub fn sign_request(
    credentials: &BedrockCredentials,
    region: &str,
    method: &str,
    url: &str,
    body: &str,
    streaming: bool,
) -> Result<Vec<(String, String)>, LlmError> {
    // Parse the URL
    let parsed_url = parse_url(url)?;
    let host = parsed_url.host;
    let path = parsed_url.path;
    let query = parsed_url.query;

    // Get current timestamp
    let now = chrono::Utc::now();
    let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
    let date_stamp = now.format("%Y%m%d").to_string();

    // Calculate content hash
    let payload_hash = sha256_hex(body.as_bytes());

    // Build canonical headers (request body is always JSON, even for streaming)
    let content_type = CONTENT_TYPE_JSON;

    let accept = if streaming {
        CONTENT_TYPE_EVENT_STREAM
    } else {
        CONTENT_TYPE_JSON
    };

    // Headers that will be signed (must be in lowercase, sorted alphabetically)
    let mut headers = vec![
        ("accept".to_string(), accept.to_string()),
        ("content-type".to_string(), content_type.to_string()),
        ("host".to_string(), host.clone()),
        ("x-amz-content-sha256".to_string(), payload_hash.clone()),
        ("x-amz-date".to_string(), amz_date.clone()),
    ];

    // Add session token if present
    if let Some(token) = &credentials.session_token {
        headers.push(("x-amz-security-token".to_string(), token.clone()));
    }

    // Sort headers by name
    headers.sort_by(|a, b| a.0.cmp(&b.0));

    // Build canonical headers string
    let canonical_headers: String = headers
        .iter()
        .map(|(k, v)| format!("{}:{}\n", k, v.trim()))
        .collect();

    // Build signed headers string
    let signed_headers: String = headers
        .iter()
        .map(|(k, _)| k.as_str())
        .collect::<Vec<_>>()
        .join(";");

    // Build canonical request
    let canonical_request = format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        method,
        path,
        query.as_deref().unwrap_or(""),
        canonical_headers,
        signed_headers,
        payload_hash
    );

    // Create string to sign
    let credential_scope = format!(
        "{}/{}/{}/{}",
        date_stamp, region, SERVICE_NAME, AWS4_REQUEST
    );
    let canonical_request_hash = sha256_hex(canonical_request.as_bytes());
    let string_to_sign = format!(
        "{}\n{}\n{}\n{}",
        AWS_ALGORITHM, amz_date, credential_scope, canonical_request_hash
    );

    // Calculate signature
    let signing_key = get_signature_key(
        &credentials.secret_access_key,
        &date_stamp,
        region,
        SERVICE_NAME,
    );
    let signature = hmac_sha256_hex(&signing_key, string_to_sign.as_bytes());

    // Build authorization header
    let authorization = format!(
        "{} Credential={}/{}, SignedHeaders={}, Signature={}",
        AWS_ALGORITHM, credentials.access_key_id, credential_scope, signed_headers, signature
    );

    // Build final headers
    let mut result_headers = vec![
        ("Authorization".to_string(), authorization),
        ("Accept".to_string(), accept.to_string()),
        ("Content-Type".to_string(), content_type.to_string()),
        ("Host".to_string(), host),
        ("X-Amz-Content-Sha256".to_string(), payload_hash),
        ("X-Amz-Date".to_string(), amz_date),
    ];

    if let Some(token) = &credentials.session_token {
        result_headers.push(("X-Amz-Security-Token".to_string(), token.clone()));
    }

    Ok(result_headers)
}

// =============================================================================
// Private Helpers
// =============================================================================

/// Parsed URL components.
struct ParsedUrl {
    host: String,
    path: String,
    query: Option<String>,
}

/// Parse a URL into its components.
fn parse_url(url: &str) -> Result<ParsedUrl, LlmError> {
    // Simple URL parsing - assumes https://host/path?query format
    let url = url
        .strip_prefix("https://")
        .ok_or_else(|| LlmError::new("INVALID_URL", "URL must start with https://"))?;

    let (host_and_path, query) = match url.split_once('?') {
        Some((hp, q)) => (hp, Some(q.to_string())),
        None => (url, None),
    };

    let (host, path) = match host_and_path.split_once('/') {
        Some((h, p)) => (h.to_string(), format!("/{}", p)),
        None => (host_and_path.to_string(), "/".to_string()),
    };

    Ok(ParsedUrl { host, path, query })
}

/// Calculate SHA256 hash and return as hex string.
fn sha256_hex(data: &[u8]) -> String {
    let hash = digest::digest(&digest::SHA256, data);
    hex::encode(hash.as_ref())
}

/// Calculate HMAC-SHA256 and return as hex string.
fn hmac_sha256_hex(key: &[u8], data: &[u8]) -> String {
    let key = hmac::Key::new(hmac::HMAC_SHA256, key);
    let tag = hmac::sign(&key, data);
    hex::encode(tag.as_ref())
}

/// Calculate HMAC-SHA256 and return raw bytes.
fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let key = hmac::Key::new(hmac::HMAC_SHA256, key);
    let tag = hmac::sign(&key, data);
    tag.as_ref().to_vec()
}

/// Derive the AWS SigV4 signing key.
fn get_signature_key(secret_key: &str, date_stamp: &str, region: &str, service: &str) -> Vec<u8> {
    let k_date = hmac_sha256(
        format!("{}{}", AWS4_PREFIX, secret_key).as_bytes(),
        date_stamp.as_bytes(),
    );
    let k_region = hmac_sha256(&k_date, region.as_bytes());
    let k_service = hmac_sha256(&k_region, service.as_bytes());
    hmac_sha256(&k_service, AWS4_REQUEST.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_url_simple() {
        let parsed =
            parse_url("https://bedrock-runtime.us-east-1.amazonaws.com/model/test/converse")
                .unwrap();
        assert_eq!(parsed.host, "bedrock-runtime.us-east-1.amazonaws.com");
        assert_eq!(parsed.path, "/model/test/converse");
        assert!(parsed.query.is_none());
    }

    #[test]
    fn test_sha256_hex() {
        // Empty string hash
        let hash = sha256_hex(b"");
        assert_eq!(
            hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn test_sign_request() {
        let credentials = BedrockCredentials::new(
            "AKIAIOSFODNN7EXAMPLE",
            "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
        );
        let url = "https://bedrock-runtime.us-east-1.amazonaws.com/model/anthropic.claude-3-sonnet-20240229-v1:0/converse";
        let body = r#"{"messages":[]}"#;

        let headers = sign_request(&credentials, "us-east-1", "POST", url, body, false).unwrap();

        // Verify we get the expected headers
        let header_names: Vec<&str> = headers.iter().map(|(k, _)| k.as_str()).collect();
        assert!(header_names.contains(&"Authorization"));
        assert!(header_names.contains(&"X-Amz-Date"));
        assert!(header_names.contains(&"X-Amz-Content-Sha256"));
        assert!(header_names.contains(&"Host"));
    }
}
