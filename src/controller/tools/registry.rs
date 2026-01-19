use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;

use super::types::Executable;

/// Thread-safe registry for managing available tools.
pub struct ToolRegistry {
    tools: RwLock<HashMap<String, Arc<dyn Executable>>>,
}

impl ToolRegistry {
    /// Create a new empty tool registry.
    pub fn new() -> Self {
        Self {
            tools: RwLock::new(HashMap::new()),
        }
    }

    /// Register a tool in the registry.
    /// Returns an error if a tool with the same name already exists.
    pub async fn register(&self, tool: Arc<dyn Executable>) -> Result<(), String> {
        let name = tool.name().to_string();
        let mut tools = self.tools.write().await;

        if tools.contains_key(&name) {
            return Err(format!("tool with name {:?} already exists", name));
        }

        tools.insert(name, tool);
        Ok(())
    }

    /// Get a tool by name.
    /// Returns None if the tool is not found.
    pub async fn get(&self, name: &str) -> Option<Arc<dyn Executable>> {
        let tools = self.tools.read().await;
        tools.get(name).cloned()
    }

    /// Check if a tool exists in the registry.
    pub async fn has(&self, name: &str) -> bool {
        let tools = self.tools.read().await;
        tools.contains_key(name)
    }

    /// Remove a tool from the registry.
    pub async fn remove(&self, name: &str) {
        let mut tools = self.tools.write().await;
        tools.remove(name);
    }

    /// List all registered tool names.
    pub async fn list(&self) -> Vec<String> {
        let tools = self.tools.read().await;
        tools.keys().cloned().collect()
    }

    /// Get all registered tools.
    pub async fn get_all(&self) -> Vec<Arc<dyn Executable>> {
        let tools = self.tools.read().await;
        tools.values().cloned().collect()
    }

    /// Get the number of registered tools.
    pub async fn len(&self) -> usize {
        let tools = self.tools.read().await;
        tools.len()
    }

    /// Check if the registry is empty.
    pub async fn is_empty(&self) -> bool {
        let tools = self.tools.read().await;
        tools.is_empty()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::types::{ToolContext, ToolType};
    use std::pin::Pin;
    use std::future::Future;

    struct MockTool {
        name: String,
    }

    impl Executable for MockTool {
        fn name(&self) -> &str {
            &self.name
        }

        fn description(&self) -> &str {
            "A mock tool for testing"
        }

        fn input_schema(&self) -> &str {
            r#"{"type":"object"}"#
        }

        fn tool_type(&self) -> ToolType {
            ToolType::Custom
        }

        fn execute(
            &self,
            _context: ToolContext,
            _input: HashMap<String, serde_json::Value>,
        ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send>> {
            Box::pin(async { Ok("mock result".to_string()) })
        }
    }

    #[tokio::test]
    async fn test_register_and_get() {
        let registry = ToolRegistry::new();
        let tool = Arc::new(MockTool {
            name: "test_tool".to_string(),
        });

        registry.register(tool).await.unwrap();

        let retrieved = registry.get("test_tool").await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name(), "test_tool");
    }

    #[tokio::test]
    async fn test_duplicate_registration() {
        let registry = ToolRegistry::new();
        let tool1 = Arc::new(MockTool {
            name: "test_tool".to_string(),
        });
        let tool2 = Arc::new(MockTool {
            name: "test_tool".to_string(),
        });

        registry.register(tool1).await.unwrap();
        let result = registry.register(tool2).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_list_and_remove() {
        let registry = ToolRegistry::new();
        let tool = Arc::new(MockTool {
            name: "test_tool".to_string(),
        });

        registry.register(tool).await.unwrap();
        assert!(registry.has("test_tool").await);

        let names = registry.list().await;
        assert_eq!(names.len(), 1);

        registry.remove("test_tool").await;
        assert!(!registry.has("test_tool").await);
    }
}
