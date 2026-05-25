use async_trait::async_trait;
use scraper::{Html, Selector};
use serde_json::json;
use std::sync::Arc;
use zeroclaw_api::tool::{Tool, ToolResult};
use zeroclaw_config::policy::SecurityPolicy;

/// Tool for extracting structured text or HTML snippets from raw HTML using CSS selectors.
pub struct WebParseTool {}

impl WebParseTool {
    pub fn new(_security: Arc<SecurityPolicy>) -> Self {
        Self {}
    }
}

#[async_trait]
impl Tool for WebParseTool {
    fn name(&self) -> &str {
        "web_parse"
    }

    fn description(&self) -> &str {
        "Parse raw HTML text and extract matching elements and text using CSS selectors."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "html": {
                    "type": "string",
                    "description": "The raw HTML string to parse"
                },
                "selector": {
                    "type": "string",
                    "description": "CSS selector to search for (e.g. 'div.content', 'h1', 'a.link')"
                }
            },
            "required": ["html", "selector"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let html_content = match args.get("html").and_then(|h| h.as_str()) {
            Some(h) => h,
            None => {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some("Missing html parameter".into()),
                });
            }
        };

        let selector_str = match args.get("selector").and_then(|s| s.as_str()) {
            Some(s) => s,
            None => {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some("Missing selector parameter".into()),
                });
            }
        };

        let document = Html::parse_document(html_content);
        let selector = match Selector::parse(selector_str) {
            Ok(s) => s,
            Err(e) => {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(format!("Invalid CSS selector: {e:?}")),
                });
            }
        };

        let mut output = String::new();
        let mut count = 0;

        for element in document.select(&selector) {
            count += 1;
            let text: String = element.text().collect::<Vec<_>>().join(" ");
            let html = element.html();

            output.push_str(&format!("Match #{count}:\n"));
            output.push_str(&format!("- Text: {}\n", text.trim()));
            output.push_str(&format!("- HTML: {}\n\n", html.trim()));

            if count >= 30 {
                output.push_str("... (truncated after 30 matches)\n");
                break;
            }
        }

        if count == 0 {
            Ok(ToolResult {
                success: true,
                output: format!("No elements found matching CSS selector: '{selector_str}'"),
                error: None,
            })
        } else {
            Ok(ToolResult {
                success: true,
                output,
                error: None,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zeroclaw_config::autonomy::AutonomyLevel;

    fn test_security() -> Arc<SecurityPolicy> {
        Arc::new(SecurityPolicy {
            autonomy: AutonomyLevel::Full,
            workspace_dir: std::env::temp_dir(),
            ..SecurityPolicy::default()
        })
    }

    #[test]
    fn web_parse_tool_metadata() {
        let tool = WebParseTool::new(test_security());
        assert_eq!(tool.name(), "web_parse");
    }

    #[tokio::test]
    async fn test_web_parse_execution() {
        let tool = WebParseTool::new(test_security());
        let html =
            r#"<html><body><h1 class="title">Hello World</h1><p>Paragraph</p></body></html>"#;
        let result = tool
            .execute(json!({
                "html": html,
                "selector": "h1.title"
            }))
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.output.contains("Hello World"));
        assert!(result.output.contains("h1 class=\"title\""));
    }
}
