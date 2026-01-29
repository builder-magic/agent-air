//! Gemini API request/response types and serialization.

use crate::client::error::LlmError;
use crate::client::models::{
    Content, GroundingChunk, GroundingMetadata, GroundingSupport, Message, MessageOptions,
    ResponseMetadata, Role, SafetyRating, ToolChoice, ToolUse,
};
use crate::client::providers::common::{
    escape_json_string, extract_text_content, generate_unique_id,
};

// =============================================================================
// Constants
// =============================================================================

/// Base URL for Gemini API.
const API_BASE: &str = "https://generativelanguage.googleapis.com/v1beta/models";

/// API action for content generation.
const ACTION_GENERATE_CONTENT: &str = ":generateContent";

/// API action for streaming content generation.
const ACTION_STREAM_GENERATE: &str = ":streamGenerateContent?alt=sse";

/// HTTP header for content type.
const HEADER_CONTENT_TYPE: &str = "Content-Type";

/// HTTP header for API key authentication.
const HEADER_API_KEY: &str = "x-goog-api-key";

/// Content type for JSON requests.
const CONTENT_TYPE_JSON: &str = "application/json";

/// Gemini role for user messages.
const ROLE_USER: &str = "user";

/// Gemini role for model/assistant messages.
const ROLE_MODEL: &str = "model";

/// Tool calling mode: model decides whether to use tools.
const TOOL_MODE_AUTO: &str = "AUTO";

/// Tool calling mode: model must use at least one tool.
const TOOL_MODE_ANY: &str = "ANY";

/// Tool calling mode: model cannot use tools.
const TOOL_MODE_NONE: &str = "NONE";

/// Error code for invalid request.
const ERROR_INVALID_REQUEST: &str = "INVALID_REQUEST";

/// Error code for parse errors.
const ERROR_PARSE: &str = "PARSE_ERROR";

/// Prefix for Gemini API error codes.
const ERROR_PREFIX_GEMINI: &str = "GEMINI_ERROR_";

/// Prefix for generated tool call IDs.
const TOOL_CALL_ID_PREFIX: &str = "gemini_call_";

/// Default error message when error details are unavailable.
const MSG_UNKNOWN_ERROR: &str = "Unknown error";

/// Error message when no user/assistant messages provided.
const MSG_NO_MESSAGES: &str = "At least one user or assistant message is required";

/// Error message when API key is empty.
const MSG_EMPTY_API_KEY: &str = "API key cannot be empty";

/// Error message when no candidates in response.
const MSG_NO_CANDIDATES: &str = "No candidates found in response";

/// Error message when no content in response.
const MSG_NO_CONTENT: &str = "No content found in response";

/// Error code for content blocked by safety filters.
const ERROR_CONTENT_BLOCKED: &str = "CONTENT_BLOCKED";

/// Error code for prompt blocked by safety filters.
const ERROR_PROMPT_BLOCKED: &str = "PROMPT_BLOCKED";

/// Error code for unsupported image source.
const ERROR_IMAGE_NOT_SUPPORTED: &str = "IMAGE_NOT_SUPPORTED";

/// Error message for unsupported images.
const MSG_IMAGE_NOT_SUPPORTED: &str = "Image content is not yet supported for Gemini provider. See: https://github.com/deepmesa/agent-core-ai/issues/2";

// =============================================================================
// Public API
// =============================================================================

/// Returns the Gemini API endpoint URL for content generation.
pub fn get_api_url(model: &str) -> String {
    format!("{}/{}{}", API_BASE, model, ACTION_GENERATE_CONTENT)
}

/// Returns the Gemini API endpoint URL for streaming content generation.
pub fn get_streaming_api_url(model: &str) -> String {
    format!("{}/{}{}", API_BASE, model, ACTION_STREAM_GENERATE)
}

/// Returns the HTTP headers required for the Gemini API.
///
/// # Errors
/// Returns an error if the API key is empty.
pub fn get_request_headers(api_key: &str) -> Result<Vec<(&'static str, String)>, LlmError> {
    if api_key.is_empty() {
        return Err(LlmError::new(ERROR_INVALID_REQUEST, MSG_EMPTY_API_KEY));
    }
    Ok(vec![
        (HEADER_CONTENT_TYPE, CONTENT_TYPE_JSON.to_string()),
        (HEADER_API_KEY, api_key.to_string()),
    ])
}

/// Builds the JSON request body for the Gemini generateContent API.
///
/// Gemini uses `contents[]` with `parts[]` format. System messages are
/// extracted and placed in the `systemInstruction` field.
///
/// This function is used for both streaming and non-streaming requests,
/// as Gemini uses the same request body format for both.
pub fn build_request_body(
    messages: &[Message],
    options: &MessageOptions,
) -> Result<String, LlmError> {
    let mut system_instruction: Option<String> = None;
    let mut contents: Vec<&Message> = Vec::new();

    // Separate system messages from conversation messages
    for msg in messages {
        match msg.role {
            Role::System => {
                let text = extract_text_content(msg);
                if let Some(existing) = system_instruction.take() {
                    system_instruction = Some(format!("{}\n{}", existing, text));
                } else {
                    system_instruction = Some(text);
                }
            }
            Role::User | Role::Assistant => {
                contents.push(msg);
            }
        }
    }

    if contents.is_empty() {
        return Err(LlmError::new(ERROR_INVALID_REQUEST, MSG_NO_MESSAGES));
    }

    let mut json = String::with_capacity(2048);
    json.push('{');

    // System instruction (optional)
    if let Some(system) = &system_instruction {
        json.push_str(&format!(
            r#""systemInstruction":{{"parts":[{{"text":"{}"}}]}}"#,
            escape_json_string(system)
        ));
        json.push(',');
    }

    // Contents array
    json.push_str(r#""contents":["#);
    for (i, msg) in contents.iter().enumerate() {
        if i > 0 {
            json.push(',');
        }
        json.push_str(&format_content(msg)?);
    }
    json.push(']');

    // Generation config (only if there are options to include)
    let config_items = build_generation_config(options);
    if !config_items.is_empty() {
        json.push_str(r#","generationConfig":{"#);
        json.push_str(&config_items.join(","));
        json.push('}');
    }

    // Tools (optional)
    if let Some(tools) = &options.tools {
        if !tools.is_empty() {
            json.push_str(r#","tools":[{"functionDeclarations":["#);
            for (i, tool) in tools.iter().enumerate() {
                if i > 0 {
                    json.push(',');
                }
                json.push_str(&format!(
                    r#"{{"name":"{}","description":"{}","parameters":{}}}"#,
                    escape_json_string(&tool.name),
                    escape_json_string(&tool.description),
                    tool.input_schema
                ));
            }
            json.push_str("]}]");
        }
    }

    // Tool config (optional)
    if let Some(tool_choice) = &options.tool_choice {
        let tool_config = match tool_choice {
            ToolChoice::Auto => {
                format!(
                    r#","toolConfig":{{"functionCallingConfig":{{"mode":"{}"}}}}"#,
                    TOOL_MODE_AUTO
                )
            }
            ToolChoice::Any => {
                format!(
                    r#","toolConfig":{{"functionCallingConfig":{{"mode":"{}"}}}}"#,
                    TOOL_MODE_ANY
                )
            }
            ToolChoice::None => {
                format!(
                    r#","toolConfig":{{"functionCallingConfig":{{"mode":"{}"}}}}"#,
                    TOOL_MODE_NONE
                )
            }
            ToolChoice::Tool(name) => {
                format!(
                    r#","toolConfig":{{"functionCallingConfig":{{"mode":"{}","allowedFunctionNames":["{}"]}}}}"#,
                    TOOL_MODE_ANY,
                    escape_json_string(name)
                )
            }
        };
        json.push_str(&tool_config);
    }

    json.push('}');

    Ok(json)
}

/// Parses the Gemini API response and extracts the assistant message.
///
/// This function handles:
/// - API error responses
/// - Prompt feedback (blocked prompts) with meaningful error messages
/// - Content extraction from candidates
/// - Safety ratings metadata
/// - Grounding/citation metadata
pub fn parse_response(response_body: &str) -> Result<Message, LlmError> {
    let parsed: serde_json::Value = serde_json::from_str(response_body)
        .map_err(|e| LlmError::new(ERROR_PARSE, format!("Failed to parse response: {}", e)))?;

    // Check for API error response
    if let Some(error) = parsed.get("error") {
        let error_code = error["code"].as_i64().unwrap_or(0);
        let error_msg = error["message"].as_str().unwrap_or(MSG_UNKNOWN_ERROR);
        return Err(LlmError::new(
            format!("{}{}", ERROR_PREFIX_GEMINI, error_code),
            error_msg,
        ));
    }

    // Check for prompt feedback (blocked prompts) - Item 12 fix
    if let Some(prompt_feedback) = parsed.get("promptFeedback") {
        if let Some(block_reason) = prompt_feedback["blockReason"].as_str() {
            let safety_info = extract_safety_info_from_feedback(prompt_feedback);
            let error_msg = format!(
                "Prompt blocked by Gemini safety filters. Reason: {}. {}",
                block_reason, safety_info
            );
            return Err(LlmError::new(ERROR_PROMPT_BLOCKED, error_msg));
        }
    }

    // Extract content from successful response
    // Gemini returns: { "candidates": [{ "content": { "parts": [...], "role": "model" } }] }
    let candidates = &parsed["candidates"];

    if !candidates.is_array() || candidates.as_array().map_or(true, |a| a.is_empty()) {
        return Err(LlmError::new(ERROR_PARSE, MSG_NO_CANDIDATES));
    }

    let candidate = &candidates[0];

    // Check if candidate was blocked - Item 12 fix
    if let Some(finish_reason) = candidate["finishReason"].as_str() {
        if finish_reason == "SAFETY" || finish_reason == "RECITATION" || finish_reason == "OTHER" {
            let safety_info = extract_safety_ratings_text(candidate);
            let error_msg = format!(
                "Response blocked by Gemini. Reason: {}. {}",
                finish_reason, safety_info
            );
            return Err(LlmError::new(ERROR_CONTENT_BLOCKED, error_msg));
        }
    }

    let content = &candidate["content"];
    let parts = &content["parts"];

    let mut content_blocks: Vec<Content> = Vec::new();

    if let Some(parts_array) = parts.as_array() {
        for part in parts_array {
            // Text part
            if let Some(text) = part["text"].as_str() {
                content_blocks.push(Content::Text(text.to_string()));
            }
            // Function call part
            else if let Some(function_call) = part.get("functionCall") {
                let name = function_call["name"].as_str().unwrap_or("").to_string();
                let args = function_call["args"].to_string();
                let id = generate_unique_id(TOOL_CALL_ID_PREFIX);
                content_blocks.push(Content::ToolUse(ToolUse {
                    id,
                    name,
                    input: args,
                }));
            }
        }
    }

    if content_blocks.is_empty() {
        return Err(LlmError::new(ERROR_PARSE, MSG_NO_CONTENT));
    }

    // Extract metadata - Items 11 and 14 fix
    let response_metadata = extract_response_metadata(candidate, &parsed);

    if response_metadata.safety_ratings.is_some() || response_metadata.grounding.is_some() {
        Ok(Message::with_metadata(
            Role::Assistant,
            content_blocks,
            response_metadata,
        ))
    } else {
        Ok(Message::with_content(Role::Assistant, content_blocks))
    }
}

/// Extract safety information from prompt feedback for error messages.
fn extract_safety_info_from_feedback(feedback: &serde_json::Value) -> String {
    let mut info = Vec::new();

    if let Some(ratings) = feedback["safetyRatings"].as_array() {
        for rating in ratings {
            let category = rating["category"].as_str().unwrap_or("UNKNOWN");
            let probability = rating["probability"].as_str().unwrap_or("UNKNOWN");
            if probability != "NEGLIGIBLE" && probability != "LOW" {
                info.push(format!("{}: {}", category, probability));
            }
        }
    }

    if info.is_empty() {
        String::new()
    } else {
        format!("Safety concerns: {}", info.join(", "))
    }
}

/// Extract safety ratings text from candidate for error messages.
fn extract_safety_ratings_text(candidate: &serde_json::Value) -> String {
    let mut info = Vec::new();

    if let Some(ratings) = candidate["safetyRatings"].as_array() {
        for rating in ratings {
            let category = rating["category"].as_str().unwrap_or("UNKNOWN");
            let probability = rating["probability"].as_str().unwrap_or("UNKNOWN");
            let blocked = rating["blocked"].as_bool().unwrap_or(false);
            if blocked || probability == "HIGH" || probability == "MEDIUM" {
                info.push(format!("{}: {}", category, probability));
            }
        }
    }

    if info.is_empty() {
        String::new()
    } else {
        format!("Safety concerns: {}", info.join(", "))
    }
}

/// Extract response metadata (safety ratings, grounding) from the response.
fn extract_response_metadata(
    candidate: &serde_json::Value,
    response: &serde_json::Value,
) -> ResponseMetadata {
    let safety_ratings = extract_safety_ratings(candidate);
    let grounding = extract_grounding_metadata(candidate, response);

    ResponseMetadata {
        safety_ratings,
        grounding,
    }
}

/// Extract safety ratings from candidate - Item 11 fix.
fn extract_safety_ratings(candidate: &serde_json::Value) -> Option<Vec<SafetyRating>> {
    let ratings = candidate["safetyRatings"].as_array()?;

    let result: Vec<SafetyRating> = ratings
        .iter()
        .map(|r| SafetyRating {
            category: r["category"].as_str().unwrap_or("UNKNOWN").to_string(),
            probability: r["probability"].as_str().unwrap_or("UNKNOWN").to_string(),
            blocked: r["blocked"].as_bool().unwrap_or(false),
        })
        .collect();

    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

/// Extract grounding metadata from candidate - Item 14 fix.
fn extract_grounding_metadata(
    candidate: &serde_json::Value,
    _response: &serde_json::Value,
) -> Option<GroundingMetadata> {
    let grounding = candidate.get("groundingMetadata")?;

    let web_search_queries: Vec<String> = grounding["webSearchQueries"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let grounding_chunks: Vec<GroundingChunk> = grounding["groundingChunks"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|chunk| {
                    // Handle web chunks
                    if let Some(web) = chunk.get("web") {
                        GroundingChunk {
                            source_type: "web".to_string(),
                            uri: web["uri"].as_str().map(|s| s.to_string()),
                            title: web["title"].as_str().map(|s| s.to_string()),
                        }
                    } else {
                        GroundingChunk {
                            source_type: "unknown".to_string(),
                            uri: None,
                            title: None,
                        }
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    let grounding_supports: Vec<GroundingSupport> = grounding["groundingSupports"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|support| {
                    let segment = support.get("segment")?;
                    let start_index = segment["startIndex"].as_u64().unwrap_or(0) as usize;
                    let end_index = segment["endIndex"].as_u64().unwrap_or(0) as usize;

                    let chunk_indices: Vec<usize> = support["groundingChunkIndices"]
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_u64().map(|n| n as usize))
                                .collect()
                        })
                        .unwrap_or_default();

                    let confidence_scores: Vec<f32> = support["confidenceScores"]
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_f64().map(|n| n as f32))
                                .collect()
                        })
                        .unwrap_or_default();

                    Some(GroundingSupport {
                        start_index,
                        end_index,
                        chunk_indices,
                        confidence_scores,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    // Only return Some if there's actual grounding data
    if web_search_queries.is_empty() && grounding_chunks.is_empty() && grounding_supports.is_empty()
    {
        None
    } else {
        Some(GroundingMetadata {
            web_search_queries,
            grounding_chunks,
            grounding_supports,
        })
    }
}

// =============================================================================
// Private helpers
// =============================================================================

/// Build generation config items from options.
fn build_generation_config(options: &MessageOptions) -> Vec<String> {
    let mut config_items = Vec::new();

    if let Some(max_tokens) = options.max_tokens {
        config_items.push(format!(r#""maxOutputTokens":{}"#, max_tokens));
    }

    if let Some(temp) = options.temperature {
        config_items.push(format!(r#""temperature":{}"#, temp));
    }

    if let Some(top_p) = options.top_p {
        config_items.push(format!(r#""topP":{}"#, top_p));
    }

    if let Some(top_k) = options.top_k {
        config_items.push(format!(r#""topK":{}"#, top_k));
    }

    if let Some(stop_sequences) = &options.stop_sequences {
        if !stop_sequences.is_empty() {
            let stops: Vec<String> = stop_sequences
                .iter()
                .map(|s| format!(r#""{}""#, escape_json_string(s)))
                .collect();
            config_items.push(format!(r#""stopSequences":[{}]"#, stops.join(",")));
        }
    }

    config_items
}

fn format_content(msg: &Message) -> Result<String, LlmError> {
    let role = match msg.role {
        Role::User => ROLE_USER,
        Role::Assistant => ROLE_MODEL,
        Role::System => ROLE_USER, // Should not happen, but fallback
    };

    let mut json = format!(r#"{{"role":"{}","parts":["#, role);

    for (i, content) in msg.content.iter().enumerate() {
        if i > 0 {
            json.push(',');
        }
        json.push_str(&format_part(content)?);
    }

    json.push_str("]}");
    Ok(json)
}

fn format_part(content: &Content) -> Result<String, LlmError> {
    match content {
        Content::Text(text) => Ok(format!(r#"{{"text":"{}"}}"#, escape_json_string(text))),
        Content::Image(_) => {
            // Image support for Gemini requires either:
            // 1. Base64 inlineData (needs fetch + encode for URLs)
            // 2. Google Cloud Storage URIs (gs://) via fileData
            // Neither is currently implemented.
            Err(LlmError::new(
                ERROR_IMAGE_NOT_SUPPORTED,
                MSG_IMAGE_NOT_SUPPORTED,
            ))
        }
        Content::ToolUse(tool_use) => Ok(format!(
            r#"{{"functionCall":{{"name":"{}","args":{}}}}}"#,
            escape_json_string(&tool_use.name),
            tool_use.input
        )),
        Content::ToolResult(tool_result) => {
            // IMPORTANT: Gemini's functionResponse requires the function NAME, not a unique ID.
            // Unlike Anthropic where tool_use_id is a unique identifier (e.g., "toolu_123"),
            // Gemini matches responses to calls by function name.
            //
            // When using this provider, callers should set tool_result.tool_use_id to the
            // function name (e.g., "get_weather") rather than a unique call ID.
            Ok(format!(
                r#"{{"functionResponse":{{"name":"{}","response":{{"result":"{}"}}}}}}"#,
                escape_json_string(&tool_result.tool_use_id),
                escape_json_string(&tool_result.content)
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::models::Tool;

    #[test]
    fn test_get_api_url() {
        let url = get_api_url("gemini-1.5-pro");
        assert_eq!(
            url,
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-1.5-pro:generateContent"
        );
    }

    #[test]
    fn test_get_streaming_api_url() {
        let url = get_streaming_api_url("gemini-1.5-flash");
        assert_eq!(
            url,
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-1.5-flash:streamGenerateContent?alt=sse"
        );
    }

    #[test]
    fn test_get_request_headers_valid() {
        let headers = get_request_headers("test-api-key").unwrap();
        assert_eq!(headers.len(), 2);
        assert_eq!(
            headers[0],
            (HEADER_CONTENT_TYPE, CONTENT_TYPE_JSON.to_string())
        );
        assert_eq!(headers[1], (HEADER_API_KEY, "test-api-key".to_string()));
    }

    #[test]
    fn test_get_request_headers_empty_key() {
        let result = get_request_headers("");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.error_code, ERROR_INVALID_REQUEST);
    }

    #[test]
    fn test_build_request_body_simple() {
        let messages = vec![Message::user("Hello")];
        let options = MessageOptions::default();

        let json = build_request_body(&messages, &options).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert!(parsed.get("contents").is_some());
        let contents = parsed["contents"].as_array().unwrap();
        assert_eq!(contents.len(), 1);
        assert_eq!(contents[0]["role"], "user");
        assert_eq!(contents[0]["parts"][0]["text"], "Hello");

        // Should not have empty generationConfig
        assert!(parsed.get("generationConfig").is_none());
    }

    #[test]
    fn test_build_request_body_with_system() {
        let messages = vec![Message::system("You are helpful"), Message::user("Hello")];
        let options = MessageOptions::default();

        let json = build_request_body(&messages, &options).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert!(parsed.get("systemInstruction").is_some());
        assert_eq!(
            parsed["systemInstruction"]["parts"][0]["text"],
            "You are helpful"
        );
    }

    #[test]
    fn test_build_request_body_with_options() {
        let messages = vec![Message::user("Hello")];
        let options = MessageOptions {
            max_tokens: Some(1024),
            temperature: Some(0.7),
            top_p: Some(0.9),
            top_k: Some(40),
            ..Default::default()
        };

        let json = build_request_body(&messages, &options).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        let config = &parsed["generationConfig"];
        assert_eq!(config["maxOutputTokens"], 1024);
        assert_eq!(config["temperature"], 0.7);
        assert_eq!(config["topP"], 0.9);
        assert_eq!(config["topK"], 40);
    }

    #[test]
    fn test_build_request_body_with_tools() {
        let messages = vec![Message::user("What's the weather?")];
        let options = MessageOptions {
            tools: Some(vec![Tool::new(
                "get_weather",
                "Get weather for a location",
                r#"{"type":"object","properties":{"location":{"type":"string"}},"required":["location"]}"#,
            )]),
            tool_choice: Some(ToolChoice::Auto),
            ..Default::default()
        };

        let json = build_request_body(&messages, &options).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert!(parsed.get("tools").is_some());
        let declarations = &parsed["tools"][0]["functionDeclarations"];
        assert_eq!(declarations[0]["name"], "get_weather");

        assert!(parsed.get("toolConfig").is_some());
        assert_eq!(
            parsed["toolConfig"]["functionCallingConfig"]["mode"],
            "AUTO"
        );
    }

    #[test]
    fn test_build_request_body_conversation() {
        let messages = vec![
            Message::user("Hello"),
            Message::assistant("Hi there!"),
            Message::user("How are you?"),
        ];
        let options = MessageOptions::default();

        let json = build_request_body(&messages, &options).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        let contents = parsed["contents"].as_array().unwrap();
        assert_eq!(contents.len(), 3);
        assert_eq!(contents[0]["role"], "user");
        assert_eq!(contents[1]["role"], "model");
        assert_eq!(contents[2]["role"], "user");
    }

    #[test]
    fn test_build_request_body_with_tool_result() {
        let messages = vec![
            Message::user("What's the weather in SF?"),
            Message::with_content(
                Role::Assistant,
                vec![Content::ToolUse(ToolUse {
                    id: "call_123".to_string(),
                    name: "get_weather".to_string(),
                    input: r#"{"location":"San Francisco"}"#.to_string(),
                })],
            ),
            // Note: For Gemini, tool_use_id should be the function name
            Message::tool_result("get_weather", "72F and sunny", false),
        ];
        let options = MessageOptions::default();

        let json = build_request_body(&messages, &options).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        let contents = parsed["contents"].as_array().unwrap();
        assert_eq!(contents.len(), 3);

        // Check function call
        assert!(contents[1]["parts"][0].get("functionCall").is_some());

        // Check function response
        assert!(contents[2]["parts"][0].get("functionResponse").is_some());
        assert_eq!(
            contents[2]["parts"][0]["functionResponse"]["name"],
            "get_weather"
        );
    }

    #[test]
    fn test_parse_response_text() {
        let response = r#"{
            "candidates": [{
                "content": {
                    "parts": [{"text": "Hello! How can I help you?"}],
                    "role": "model"
                },
                "finishReason": "STOP"
            }]
        }"#;

        let msg = parse_response(response).unwrap();
        assert_eq!(msg.role, Role::Assistant);
        match &msg.content[0] {
            Content::Text(t) => assert_eq!(t, "Hello! How can I help you?"),
            _ => panic!("Expected text content"),
        }
    }

    #[test]
    fn test_parse_response_function_call() {
        let response = r#"{
            "candidates": [{
                "content": {
                    "parts": [
                        {"text": "Let me check the weather."},
                        {"functionCall": {"name": "get_weather", "args": {"location": "SF"}}}
                    ],
                    "role": "model"
                },
                "finishReason": "STOP"
            }]
        }"#;

        let msg = parse_response(response).unwrap();
        assert_eq!(msg.role, Role::Assistant);
        assert_eq!(msg.content.len(), 2);

        match &msg.content[0] {
            Content::Text(t) => assert_eq!(t, "Let me check the weather."),
            _ => panic!("Expected text content"),
        }

        match &msg.content[1] {
            Content::ToolUse(tu) => {
                assert_eq!(tu.name, "get_weather");
                assert!(tu.id.starts_with(TOOL_CALL_ID_PREFIX));
            }
            _ => panic!("Expected tool use content"),
        }
    }

    #[test]
    fn test_parse_response_error() {
        let response = r#"{
            "error": {
                "code": 400,
                "message": "Invalid API key provided",
                "status": "INVALID_ARGUMENT"
            }
        }"#;

        let err = parse_response(response).unwrap_err();
        assert!(err.error_code.starts_with(ERROR_PREFIX_GEMINI));
        assert!(err.error_message.contains("Invalid API key"));
    }

    #[test]
    fn test_build_request_body_empty_messages() {
        let messages: Vec<Message> = vec![];
        let options = MessageOptions::default();

        let result = build_request_body(&messages, &options);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.error_code, ERROR_INVALID_REQUEST);
    }

    #[test]
    fn test_build_request_body_system_only() {
        let messages = vec![Message::system("You are helpful")];
        let options = MessageOptions::default();

        let result = build_request_body(&messages, &options);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.error_code, ERROR_INVALID_REQUEST);
    }

    #[test]
    fn test_parse_response_prompt_blocked() {
        let response = r#"{
            "promptFeedback": {
                "blockReason": "SAFETY",
                "safetyRatings": [
                    {"category": "HARM_CATEGORY_SEXUALLY_EXPLICIT", "probability": "HIGH"}
                ]
            }
        }"#;

        let err = parse_response(response).unwrap_err();
        assert_eq!(err.error_code, ERROR_PROMPT_BLOCKED);
        assert!(err.error_message.contains("SAFETY"));
        assert!(
            err.error_message
                .contains("HARM_CATEGORY_SEXUALLY_EXPLICIT")
        );
    }

    #[test]
    fn test_parse_response_content_blocked() {
        let response = r#"{
            "candidates": [{
                "content": {
                    "parts": [{"text": ""}],
                    "role": "model"
                },
                "finishReason": "SAFETY",
                "safetyRatings": [
                    {"category": "HARM_CATEGORY_DANGEROUS_CONTENT", "probability": "HIGH", "blocked": true}
                ]
            }]
        }"#;

        let err = parse_response(response).unwrap_err();
        assert_eq!(err.error_code, ERROR_CONTENT_BLOCKED);
        assert!(err.error_message.contains("SAFETY"));
    }

    #[test]
    fn test_parse_response_with_safety_ratings() {
        let response = r#"{
            "candidates": [{
                "content": {
                    "parts": [{"text": "Hello!"}],
                    "role": "model"
                },
                "finishReason": "STOP",
                "safetyRatings": [
                    {"category": "HARM_CATEGORY_HARASSMENT", "probability": "NEGLIGIBLE", "blocked": false},
                    {"category": "HARM_CATEGORY_HATE_SPEECH", "probability": "LOW", "blocked": false}
                ]
            }]
        }"#;

        let msg = parse_response(response).unwrap();
        assert_eq!(msg.role, Role::Assistant);

        // Check safety ratings are included in metadata
        let metadata = msg.response_metadata.expect("Should have metadata");
        let ratings = metadata.safety_ratings.expect("Should have safety ratings");
        assert_eq!(ratings.len(), 2);
        assert_eq!(ratings[0].category, "HARM_CATEGORY_HARASSMENT");
        assert_eq!(ratings[0].probability, "NEGLIGIBLE");
        assert!(!ratings[0].blocked);
    }

    #[test]
    fn test_parse_response_with_grounding() {
        let response = r#"{
            "candidates": [{
                "content": {
                    "parts": [{"text": "The weather in SF is sunny."}],
                    "role": "model"
                },
                "finishReason": "STOP",
                "groundingMetadata": {
                    "webSearchQueries": ["weather in San Francisco"],
                    "groundingChunks": [
                        {"web": {"uri": "https://weather.com/sf", "title": "SF Weather"}}
                    ],
                    "groundingSupports": [
                        {
                            "segment": {"startIndex": 0, "endIndex": 27},
                            "groundingChunkIndices": [0],
                            "confidenceScores": [0.95]
                        }
                    ]
                }
            }]
        }"#;

        let msg = parse_response(response).unwrap();
        assert_eq!(msg.role, Role::Assistant);

        // Check grounding metadata
        let metadata = msg.response_metadata.expect("Should have metadata");
        let grounding = metadata.grounding.expect("Should have grounding");

        assert_eq!(grounding.web_search_queries.len(), 1);
        assert_eq!(grounding.web_search_queries[0], "weather in San Francisco");

        assert_eq!(grounding.grounding_chunks.len(), 1);
        assert_eq!(grounding.grounding_chunks[0].source_type, "web");
        assert_eq!(
            grounding.grounding_chunks[0].uri,
            Some("https://weather.com/sf".to_string())
        );

        assert_eq!(grounding.grounding_supports.len(), 1);
        assert_eq!(grounding.grounding_supports[0].start_index, 0);
        assert_eq!(grounding.grounding_supports[0].end_index, 27);
        assert_eq!(grounding.grounding_supports[0].chunk_indices, vec![0]);
    }
}
