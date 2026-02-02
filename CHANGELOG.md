# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Google Gemini provider support
- OpenAI streaming and OpenAI-compatible provider support with provider registry
- Bedrock, Cohere, and Azure OpenAI support
- Backpressure to controller and configurable channel sizes
- Standard tools: FileRead, DirectoryList
- GrepTool and environment context loading
- GlobTool for fast file pattern matching
- BashTool for shell command execution
- EditFileTool for find-and-replace operations
- MultiEditTool for atomic multiple find-and-replace

### Changed
- Show average similarity for fuzzy matches

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
