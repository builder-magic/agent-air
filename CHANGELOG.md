# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.6.0] - 2025-02-03

### Added

#### Event Sink Abstraction (Pluggable Frontends)
- New `interface` module for custom frontend integration
- `EventSink` trait for receiving events from the engine
  - `ChannelEventSink`: Channel-backed sink with backpressure support
  - `SimpleEventSink`: Minimal stdout sink for CLI tools
- `InputSource` trait for providing input to the engine
  - `ChannelInputSource`: Channel-backed input with helper `channel()` method
- `PermissionPolicy` trait for automatic permission handling
  - `AutoApprovePolicy`: Auto-approve all requests (headless/trusted environments)
  - `DenyAllPolicy`: Deny all requests (sandboxed environments)
  - `InteractivePolicy`: Forward all requests to user (default for TUI)
  - `PolicyDecision` enum with `Allow`, `AllowWithGrant`, `Deny`, `AskUser` variants
  - `supports_interaction()` method for handling user questions in headless mode

#### AgentAir Frontend Methods
- `run_with_frontend()`: Run agent with custom sink, source, and policy
- Inline policy handling for `PermissionRequired` and `BatchPermissionRequired` events
- Auto-cancel user interactions when policy doesn't support interaction

#### Simplified Agent Setup
- `SimpleConfig` struct for quick agent configuration
- `AgentAir::with_config(name, path, prompt)`: One-line agent creation

### Changed
- Re-export interface types at `agent` module level for convenience

## [0.5.0] - 2025-02-03

### Added

#### Agent Skills Support
- New `skills` module implementing the [Agent Skills](https://agentskills.io) open format
- `SkillDiscovery` for scanning directories for SKILL.md files
- `SkillRegistry` thread-safe registry with XML generation for system prompts
- YAML frontmatter parser with name/description validation
- Default search paths: `$PWD/.skills/` (project) and `~/.agent-air/skills/` (user)

#### AgentAir Skill Methods
- `load_skills()`: Discover and register skills from configured paths
- `load_skills_from()`: One-shot loading from custom paths
- `reload_skills()`: Hot reload with added/removed tracking
- `refresh_session_skills()`: Update session system prompts with current skills
- `add_skill_path()`: Add custom skill search directories
- `skill_registry()`: Access the skill registry directly
- Automatic skills XML injection on session creation

#### ListSkillsTool
- New tool allowing LLM to discover available skills at runtime
- Returns name, description, and SKILL.md path for each skill
- `register_list_skills_tool()` method on AgentAir

### Changed
- `AgentError` now includes `SessionNotFound` variant for skill refresh errors

## [0.4.0] - 2025-02-02

### Changed
- Split agent-air into workspace with three crates:
  - `agent-air-runtime`: Core engine (controller, client, permissions, tools)
  - `agent-air-tui`: TUI frontend (ratatui, widgets, themes, commands)
  - `agent-air`: Meta-crate re-exporting both (backwards compatible)
- TUI methods moved from AgentAir to TuiRunner
- New AgentAirExt trait provides into_tui() conversion
- Headless agents can now depend on agent-air-runtime only

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
- AgentAir::load_environment_context() for one-line enablement
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
- Initial agent-air framework with shared TUI and agent modules
- LLM controller integration (merged from llm-controller-rs)
- Anthropic client integration (merged from vangogh-rs)
- Flexible layout system for TUI widget arrangement
- Exit confirmation logic in KeyHandler
- TLS initialization and error handling
