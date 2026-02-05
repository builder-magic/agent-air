# agent-core

[![CI](https://github.com/deepmesa/agent-core/actions/workflows/ci.yml/badge.svg?branch=mainline)](https://github.com/deepmesa/agent-core/actions/workflows/ci.yml)
![License](https://img.shields.io/badge/License-Apache--2.0-blue)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

A Rust framework for building terminal UI agents powered by large language models.

agent-core provides the infrastructure to create production-ready LLM-powered terminal applications. It handles the complexity of managing async communication between UI components, LLM providers, and tool execution while offering flexible abstractions for customization.

## Features

- Complete TUI components built on Ratatui (chat views, input widgets, permission panels)
- Session management with automatic context window compaction
- Tool execution framework with concurrent execution and permission handling
- Provider-agnostic LLM client (supports Anthropic and OpenAI)
- Customizable themes, layouts, and key bindings
- Slash command system for in-app commands
- Event-driven architecture with clean component separation

## Getting Started

Add agent-core to your Cargo.toml:

```toml
[dependencies]
agent-core = "0.1.0"
```

## Basic Usage

Create a minimal agent by implementing the `AgentConfig` trait:

```rust
use agent_core::agent::{AgentConfig, AgentCore};

struct MyAgent;

impl AgentConfig for MyAgent {
    fn config_path(&self) -> &str {
        "~/.config/myagent"
    }

    fn default_system_prompt(&self) -> &str {
        "You are a helpful assistant."
    }

    fn log_prefix(&self) -> &str {
        "myagent"
    }

    fn name(&self) -> &str {
        "MyAgent"
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let agent = AgentCore::new(MyAgent)?;
    agent.run().await?;
    Ok(())
}
```

This creates a working terminal agent with chat UI, LLM integration, and tool execution capabilities.

## Architecture

The framework has four main components:

- **agent**: Orchestrates all components and provides `AgentCore` for quick setup
- **controller**: Manages LLM sessions, tool execution, and event coordination
- **client**: HTTP client for LLM providers (Anthropic, OpenAI)
- **tui**: Terminal UI components (widgets, layouts, themes, commands)

Communication flows through async channels in an event-driven architecture. The controller coordinates between the UI, LLM provider, and tool execution layers.

## Contributing

Contributions in any form (suggestions, bug reports, pull requests, and feedback) are welcome. If you've found a bug, you can submit an issue or email me at rsingh@arrsingh.com.

## License

This project is dual-licensed under the MIT LICENSE or the Apache-2 LICENSE:

- [Apache License, Version 2.0](http://www.apache.org/licenses/LICENSE-2.0)
- [MIT License](http://opensource.org/licenses/MIT)

#### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.

Contact: rsingh@arrsingh.com
