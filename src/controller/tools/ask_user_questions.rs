//! AskUserQuestions tool implementation
//!
//! This tool allows the LLM to ask the user one or more questions
//! with structured response options (single choice, multi choice, or free text).

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use serde::{Deserialize, Serialize};

use std::sync::Arc;

use super::types::{DisplayConfig, DisplayResult, Executable, ResultContentType, ToolContext, ToolType};
use super::user_interaction::UserInteractionRegistry;

/// AskUserQuestions tool name constant.
pub const ASK_USER_QUESTIONS_TOOL_NAME: &str = "ask_user_questions";

/// AskUserQuestions tool description constant.
pub const ASK_USER_QUESTIONS_TOOL_DESCRIPTION: &str =
    "Ask the user one or more questions with structured response options. \
     Supports single choice, multiple choice, and free text question types.";

/// AskUserQuestions tool JSON schema constant.
pub const ASK_USER_QUESTIONS_TOOL_SCHEMA: &str = r#"{
    "type": "object",
    "properties": {
        "questions": {
            "type": "array",
            "description": "List of questions to ask the user",
            "items": {
                "type": "object",
                "properties": {
                    "text": {
                        "type": "string",
                        "description": "The question text to display"
                    },
                    "type": {
                        "type": "string",
                        "enum": ["SingleChoice", "MultiChoice", "FreeText"],
                        "description": "The type of question"
                    },
                    "choices": {
                        "type": "array",
                        "description": "Available choices for SingleChoice/MultiChoice. User can always type a custom answer instead.",
                        "items": {
                            "type": "string",
                            "description": "Choice text to display"
                        }
                    },
                    "required": {
                        "type": "boolean",
                        "description": "Whether an answer is required"
                    },
                    "defaultValue": {
                        "type": "string",
                        "description": "Default value for FreeText questions"
                    }
                },
                "required": ["text", "type"]
            }
        }
    },
    "required": ["questions"]
}"#;

/// Question types supported by the tool.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum Question {
    /// Single choice question - user can select exactly one option.
    /// User can always type a custom answer in addition to the provided choices.
    SingleChoice {
        /// Question text to display.
        text: String,
        /// Available choices (as simple strings).
        choices: Vec<String>,
        /// Whether an answer is required.
        #[serde(default)]
        required: bool,
    },
    /// Multiple choice question - user can select zero or more options.
    /// User can always type a custom answer in addition to the provided choices.
    MultiChoice {
        /// Question text to display.
        text: String,
        /// Available choices (as simple strings).
        choices: Vec<String>,
        /// Whether an answer is required.
        #[serde(default)]
        required: bool,
    },
    /// Free text question - user can enter any text.
    FreeText {
        /// Question text to display.
        text: String,
        /// Default value for the text field.
        #[serde(default, rename = "defaultValue")]
        default_value: Option<String>,
        /// Whether an answer is required.
        #[serde(default)]
        required: bool,
    },
}

impl Question {
    /// Get the question text.
    pub fn text(&self) -> &str {
        match self {
            Question::SingleChoice { text, .. } => text,
            Question::MultiChoice { text, .. } => text,
            Question::FreeText { text, .. } => text,
        }
    }

    /// Check if the question is required.
    pub fn is_required(&self) -> bool {
        match self {
            Question::SingleChoice { required, .. } => *required,
            Question::MultiChoice { required, .. } => *required,
            Question::FreeText { required, .. } => *required,
        }
    }

    /// Get the choices for this question (empty for FreeText).
    pub fn choices(&self) -> &[String] {
        match self {
            Question::SingleChoice { choices, .. } => choices,
            Question::MultiChoice { choices, .. } => choices,
            Question::FreeText { .. } => &[],
        }
    }
}

/// Answer to a question - simplified to just question text and answer values.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Answer {
    /// The question text this answer corresponds to.
    pub question: String,
    /// The answer value(s). For SingleChoice/FreeText this will have one element.
    /// For MultiChoice this may have multiple elements.
    pub answer: Vec<String>,
}

/// Error codes for validation failures.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ValidationErrorCode {
    /// Required field was not answered.
    RequiredFieldEmpty,
    /// More than one selection for SingleChoice.
    TooManySelections,
    /// No choices provided for choice question.
    EmptyChoices,
    /// Unknown question in answer.
    UnknownQuestion,
}

impl std::fmt::Display for ValidationErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationErrorCode::RequiredFieldEmpty => write!(f, "required_field_empty"),
            ValidationErrorCode::TooManySelections => write!(f, "too_many_selections"),
            ValidationErrorCode::EmptyChoices => write!(f, "empty_choices"),
            ValidationErrorCode::UnknownQuestion => write!(f, "unknown_question"),
        }
    }
}

/// Detail about a single validation error.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ValidationErrorDetail {
    /// Question text where the error occurred.
    pub question: String,
    /// Error code.
    pub error: ValidationErrorCode,
}

/// Validation error response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ValidationError {
    /// Error type identifier.
    pub error: String,
    /// List of validation error details.
    pub details: Vec<ValidationErrorDetail>,
}

impl ValidationError {
    /// Create a new validation error.
    pub fn new(details: Vec<ValidationErrorDetail>) -> Self {
        Self {
            error: "validation_failed".to_string(),
            details,
        }
    }
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Validation failed: ")?;
        for (i, detail) in self.details.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "'{}': {}", detail.question, detail.error)?;
        }
        Ok(())
    }
}

impl std::error::Error for ValidationError {}

/// Request to ask user questions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AskUserQuestionsRequest {
    /// List of questions to ask.
    pub questions: Vec<Question>,
}

impl AskUserQuestionsRequest {
    /// Validate the request structure (before sending to user).
    pub fn validate(&self) -> Result<(), ValidationError> {
        let mut errors = Vec::new();

        for question in &self.questions {
            match question {
                Question::SingleChoice { text, choices, .. }
                | Question::MultiChoice { text, choices, .. } => {
                    // Check for empty choices
                    if choices.is_empty() {
                        errors.push(ValidationErrorDetail {
                            question: text.clone(),
                            error: ValidationErrorCode::EmptyChoices,
                        });
                    }
                }
                Question::FreeText { .. } => {
                    // No validation needed for FreeText
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(ValidationError::new(errors))
        }
    }
}

/// Response containing answers to questions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AskUserQuestionsResponse {
    /// List of answers.
    pub answers: Vec<Answer>,
}

impl AskUserQuestionsResponse {
    /// Validate the response against the request.
    pub fn validate(&self, request: &AskUserQuestionsRequest) -> Result<(), ValidationError> {
        let mut errors = Vec::new();

        // Build a map of questions by text
        let questions: HashMap<&str, &Question> =
            request.questions.iter().map(|q| (q.text(), q)).collect();

        // Track which questions have been answered
        let mut answered: std::collections::HashSet<&str> = std::collections::HashSet::new();

        for answer in &self.answers {
            // Check if question exists
            let Some(question) = questions.get(answer.question.as_str()) else {
                errors.push(ValidationErrorDetail {
                    question: answer.question.clone(),
                    error: ValidationErrorCode::UnknownQuestion,
                });
                continue;
            };

            answered.insert(answer.question.as_str());

            // Check SingleChoice has at most one selection
            if let Question::SingleChoice { .. } = question {
                if answer.answer.len() > 1 {
                    errors.push(ValidationErrorDetail {
                        question: answer.question.clone(),
                        error: ValidationErrorCode::TooManySelections,
                    });
                }
            }
        }

        // Check required questions are answered
        for question in &request.questions {
            let question_text = question.text();
            if question.is_required() {
                // Check if question was answered with non-empty content
                let has_valid_answer = self.answers.iter().any(|a| {
                    a.question == question_text && !a.answer.is_empty() && a.answer.iter().any(|s| !s.is_empty())
                });

                if !has_valid_answer {
                    errors.push(ValidationErrorDetail {
                        question: question_text.to_string(),
                        error: ValidationErrorCode::RequiredFieldEmpty,
                    });
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(ValidationError::new(errors))
        }
    }
}

/// Tool that asks the user structured questions.
pub struct AskUserQuestionsTool {
    /// Registry for managing pending user interactions.
    registry: Arc<UserInteractionRegistry>,
}

impl AskUserQuestionsTool {
    /// Create a new AskUserQuestionsTool instance.
    ///
    /// # Arguments
    /// * `registry` - The user interaction registry to use for tracking pending questions.
    pub fn new(registry: Arc<UserInteractionRegistry>) -> Self {
        Self { registry }
    }
}

impl Executable for AskUserQuestionsTool {
    fn name(&self) -> &str {
        ASK_USER_QUESTIONS_TOOL_NAME
    }

    fn description(&self) -> &str {
        ASK_USER_QUESTIONS_TOOL_DESCRIPTION
    }

    fn input_schema(&self) -> &str {
        ASK_USER_QUESTIONS_TOOL_SCHEMA
    }

    fn tool_type(&self) -> ToolType {
        ToolType::UserInteraction
    }

    fn execute(
        &self,
        context: ToolContext,
        input: HashMap<String, serde_json::Value>,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send>> {
        let registry = self.registry.clone();

        Box::pin(async move {
            // Parse the input into a request
            let questions_value = input
                .get("questions")
                .ok_or_else(|| "Missing 'questions' field".to_string())?;

            let questions: Vec<Question> = serde_json::from_value(questions_value.clone())
                .map_err(|e| format!("Failed to parse questions: {}", e))?;

            let request = AskUserQuestionsRequest { questions };

            // Validate the request
            if let Err(validation_error) = request.validate() {
                return Err(serde_json::to_string(&validation_error)
                    .unwrap_or_else(|_| validation_error.to_string()));
            }

            // Register the interaction and wait for user response
            let rx = registry
                .register(
                    context.tool_use_id,
                    context.session_id,
                    request.clone(),
                    context.turn_id,
                )
                .await
                .map_err(|e| format!("Failed to register interaction: {}", e))?;

            // Block waiting for user response
            let response = rx
                .await
                .map_err(|_| "User declined to answer".to_string())?;

            // Validate the response against the request
            if let Err(validation_error) = response.validate(&request) {
                return Err(serde_json::to_string(&validation_error)
                    .unwrap_or_else(|_| validation_error.to_string()));
            }

            // Return the validated response as JSON
            serde_json::to_string(&response)
                .map_err(|e| format!("Failed to serialize response: {}", e))
        })
    }

    fn display_config(&self) -> DisplayConfig {
        DisplayConfig {
            display_name: "Ask User Questions".to_string(),
            display_title: Box::new(|input| {
                input
                    .get("questions")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        if arr.len() == 1 {
                            "1 question".to_string()
                        } else {
                            format!("{} questions", arr.len())
                        }
                    })
                    .unwrap_or_default()
            }),
            display_content: Box::new(|input, _result| {
                let content = input
                    .get("questions")
                    .and_then(|v| v.as_array())
                    .map(|questions| {
                        questions
                            .iter()
                            .filter_map(|q| q.get("text").and_then(|t| t.as_str()))
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .unwrap_or_default();

                DisplayResult {
                    content,
                    content_type: ResultContentType::PlainText,
                    is_truncated: false,
                    full_length: 0,
                }
            }),
        }
    }

    fn compact_summary(
        &self,
        input: &HashMap<String, serde_json::Value>,
        _result: &str,
    ) -> String {
        let count = input
            .get("questions")
            .and_then(|v| v.as_array())
            .map(|arr| arr.len())
            .unwrap_or(0);
        format!("[AskUserQuestions: {} question(s)]", count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_single_choice_question() {
        let json = r#"{
            "type": "SingleChoice",
            "text": "Which database?",
            "choices": ["PostgreSQL", "MySQL", "SQLite"],
            "required": true
        }"#;

        let question: Question = serde_json::from_str(json).unwrap();
        assert_eq!(question.text(), "Which database?");
        assert!(question.is_required());

        if let Question::SingleChoice { choices, .. } = question {
            assert_eq!(choices.len(), 3);
            assert_eq!(choices[0], "PostgreSQL");
        } else {
            panic!("Expected SingleChoice");
        }
    }

    #[test]
    fn test_parse_multi_choice_question() {
        let json = r#"{
            "type": "MultiChoice",
            "text": "Which features?",
            "choices": ["Authentication", "Logging", "Caching"],
            "required": false
        }"#;

        let question: Question = serde_json::from_str(json).unwrap();
        assert_eq!(question.text(), "Which features?");
        assert!(!question.is_required());

        if let Question::MultiChoice { choices, .. } = question {
            assert_eq!(choices.len(), 3);
        } else {
            panic!("Expected MultiChoice");
        }
    }

    #[test]
    fn test_parse_free_text_question() {
        let json = r#"{
            "type": "FreeText",
            "text": "Any notes?",
            "defaultValue": "None",
            "required": false
        }"#;

        let question: Question = serde_json::from_str(json).unwrap();
        assert_eq!(question.text(), "Any notes?");

        if let Question::FreeText { default_value, .. } = question {
            assert_eq!(default_value, Some("None".to_string()));
        } else {
            panic!("Expected FreeText");
        }
    }

    #[test]
    fn test_validate_request_empty_choices() {
        let request = AskUserQuestionsRequest {
            questions: vec![Question::SingleChoice {
                text: "Question?".to_string(),
                choices: vec![],
                required: true,
            }],
        };

        let err = request.validate().unwrap_err();
        assert_eq!(err.details.len(), 1);
        assert_eq!(err.details[0].error, ValidationErrorCode::EmptyChoices);
    }

    #[test]
    fn test_validate_response_too_many_selections() {
        let request = AskUserQuestionsRequest {
            questions: vec![Question::SingleChoice {
                text: "Question?".to_string(),
                choices: vec!["A".to_string(), "B".to_string()],
                required: true,
            }],
        };

        let response = AskUserQuestionsResponse {
            answers: vec![Answer {
                question: "Question?".to_string(),
                answer: vec!["A".to_string(), "B".to_string()],
            }],
        };

        let err = response.validate(&request).unwrap_err();
        assert!(err
            .details
            .iter()
            .any(|d| d.error == ValidationErrorCode::TooManySelections));
    }

    #[test]
    fn test_validate_response_required_field_empty() {
        let request = AskUserQuestionsRequest {
            questions: vec![Question::SingleChoice {
                text: "Question?".to_string(),
                choices: vec!["A".to_string()],
                required: true,
            }],
        };

        let response = AskUserQuestionsResponse { answers: vec![] };

        let err = response.validate(&request).unwrap_err();
        assert!(err
            .details
            .iter()
            .any(|d| d.error == ValidationErrorCode::RequiredFieldEmpty));
    }

    #[test]
    fn test_validate_response_unknown_question() {
        let request = AskUserQuestionsRequest {
            questions: vec![Question::SingleChoice {
                text: "Question?".to_string(),
                choices: vec!["A".to_string()],
                required: false,
            }],
        };

        let response = AskUserQuestionsResponse {
            answers: vec![Answer {
                question: "Unknown question?".to_string(),
                answer: vec!["A".to_string()],
            }],
        };

        let err = response.validate(&request).unwrap_err();
        assert!(err
            .details
            .iter()
            .any(|d| d.error == ValidationErrorCode::UnknownQuestion));
    }

    #[test]
    fn test_validate_response_success() {
        let request = AskUserQuestionsRequest {
            questions: vec![
                Question::SingleChoice {
                    text: "Question 1?".to_string(),
                    choices: vec!["A".to_string(), "B".to_string()],
                    required: true,
                },
                Question::MultiChoice {
                    text: "Question 2?".to_string(),
                    choices: vec!["X".to_string(), "Y".to_string()],
                    required: false,
                },
                Question::FreeText {
                    text: "Question 3?".to_string(),
                    default_value: None,
                    required: false,
                },
            ],
        };

        let response = AskUserQuestionsResponse {
            answers: vec![
                Answer {
                    question: "Question 1?".to_string(),
                    answer: vec!["A".to_string()],
                },
                Answer {
                    question: "Question 2?".to_string(),
                    answer: vec!["X".to_string(), "Y".to_string()],
                },
                Answer {
                    question: "Question 3?".to_string(),
                    answer: vec!["Some notes".to_string()],
                },
            ],
        };

        assert!(response.validate(&request).is_ok());
    }

    #[test]
    fn test_answer_serialization() {
        let answer = Answer {
            question: "Which database?".to_string(),
            answer: vec!["PostgreSQL".to_string()],
        };

        let json = serde_json::to_string(&answer).unwrap();
        assert!(json.contains("question"));
        assert!(json.contains("answer"));
        assert!(json.contains("PostgreSQL"));
    }

    #[test]
    fn test_custom_answer_allowed() {
        // Users can always type a custom answer, even for choice questions
        let request = AskUserQuestionsRequest {
            questions: vec![Question::SingleChoice {
                text: "Which database?".to_string(),
                choices: vec!["PostgreSQL".to_string(), "MySQL".to_string()],
                required: true,
            }],
        };

        let response = AskUserQuestionsResponse {
            answers: vec![Answer {
                question: "Which database?".to_string(),
                answer: vec!["MongoDB".to_string()], // Custom answer not in choices
            }],
        };

        // Should succeed - custom answers are always allowed
        assert!(response.validate(&request).is_ok());
    }
}
