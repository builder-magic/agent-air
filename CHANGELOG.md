# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.4.0] - 2025-02-02

### Changed
- Split agent-core into workspace with three crates:
  - `agent-core-runtime`: Core engine (controller, client, permissions, tools)
  - `agent-core-tui`: TUI frontend (ratatui, widgets, themes, commands)
  - `agent-core`: Meta-crate re-exporting both (backwards compatible)
- TUI methods moved from AgentCore to TuiRunner
- New AgentCoreExt trait provides into_tui() conversion
- Headless agents can now depend on agent-core-runtime only

## [0.3.0]

### Added

#### LLM Provider Support
- Google Gemini provider with streaming, tool calling, safety ratings, and grounding/citation metadata
- OpenAI streaming support with SSE parser
- OpenAI-compatible provider support for 13+ providers (Groq, Together, Fireworks, Mistral, Perplexity, DeepSeek, OpenRouter, Ollama, LM Studio, Anyscale, Cerebras, SambaNova, xAI)
- Provider registry with base_url support, context limits, default models, and env var configuration
- Amazon Bedrock provider with Converse API, AWS SigV4 signing, streaming, and session token support
- Cohere provider with Chat API v2, streaming, and tool calling
- Azure OpenAI support with resource/deployment configuration and API versioning
- AI21 Labs to known providers registry

#### Tools
- GrepTool using ripgrep libraries with three output modes (files_with_matches, content, count), context lines, file type and glob filtering
- GlobTool for fast file pattern matching with globset, sorted by modification time, hidden file filtering
- BashTool for shell command execution with timeout, working directory persistence, background execution, output limiting, and dangerous command detection
- EditFileTool for find-and-replace with exact matching, replace_all, and multi-stage fuzzy matching (whitespace-insensitive, Levenshtein-based)
- MultiEditTool for atomic multiple find-and-replace with overlap detection, reverse-order application, dry-run mode
- LsTool for directory listing with filtering, sorting, and long format options
- FileRead and DirectoryRead permission categories for fine-grained file operation control
- CategorySession scope allowing users to grant permission for all operations in a category

#### Environment & Configuration
- EnvironmentContext for injecting working directory, platform, OS version, and date into system prompts
- AgentCore::load_environment_context() for one-line enablement
- LLMRegistry::with_environment_context() to inject context into all sessions
- AgentConfig::channel_buffer_size() for customizing buffer sizes

### Changed
- Streaming enabled by default for all providers (Anthropic, OpenAI, Google)
- Default channel buffer size increased from 100 to 500
- Controller now uses backpressure (blocks on full UI channel instead of dropping events)
- Fuzzy match output now shows average similarity percentage
- Permission panel UI updated with "Grant All in Category" option and new permission category icons

### Fixed
- Gemini tool calling: use function name as tool_use_id (Gemini matches by name, not unique ID)
- OpenAI streaming: add stream_options.include_usage to ensure token counts are reported for compaction logic
- Streaming cache clearing made consistent in ChatView::append_streaming()

### Removed
- Duplicate registries from LLMController (were never used)
- Dead select branches for user interaction/permission events in controller
- Duplicate escape_json_string from openai/types.rs (uses common::escape_json_string)

## [0.2.0]

### Added
- Trait-based slash command system with CommandRegistry
- ConversationView trait and NavigationHelper for widgets
- Builder patterns and custom action support for key handling
- Optional callback for dynamic processing messages
- Design note for controller event loop mutex pattern

### Changed
- Converted status bar to first-class widget
- Simplified theme system with declarative macros
- Wrapped conversation in Arc for O(1) cloning
- Changed Widget::render to take &mut self
- Replaced Result<T, String> with proper error types
- Consolidated duplicate channel size constants
- Refactored widgets to use parameters
- Split layout module into separate files
- Improved question panel UX and added from_controller_tx getter

### Documentation
- Updated README and added Rust docs

## [0.1.0]

### Added
- Initial agent-core framework with shared TUI and agent modules
- LLM controller integration (merged from llm-controller-rs)
- Anthropic client integration (merged from vangogh-rs)
- Flexible layout system for TUI widget arrangement
- Exit confirmation logic in KeyHandler
- TLS initialization and error handling
