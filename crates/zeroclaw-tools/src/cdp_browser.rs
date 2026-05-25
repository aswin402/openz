use async_trait::async_trait;
use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotParams;
use futures_util::StreamExt;
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use zeroclaw_api::tool::{Tool, ToolResult};
use zeroclaw_config::policy::SecurityPolicy;

/// Tool for controlling a headless Chrome instance via Chrome DevTools Protocol (CDP).
pub struct CdpBrowserTool {
    security: Arc<SecurityPolicy>,
}

impl CdpBrowserTool {
    pub fn new(security: Arc<SecurityPolicy>) -> Self {
        Self { security }
    }
}

#[async_trait]
impl Tool for CdpBrowserTool {
    fn name(&self) -> &str {
        "cdp_browser"
    }

    fn description(&self) -> &str {
        "Control a headless Chromium browser using Chrome DevTools Protocol (CDP) for fast automation."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["navigate", "screenshot", "click", "type", "evaluate", "interactive_elements", "visual_targets"],
                    "description": "Browser action to perform"
                },
                "url": {
                    "type": "string",
                    "description": "URL to navigate to"
                },
                "selector": {
                    "type": "string",
                    "description": "CSS selector for target element"
                },
                "text": {
                    "type": "string",
                    "description": "Text to type or JS script to evaluate"
                },
                "filename": {
                    "type": "string",
                    "description": "Filename for screenshot saving (default: cdp_screenshot.png)"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        if !self.security.can_act() {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some("Action blocked: autonomy is read-only".into()),
            });
        }

        let action = match args.get("action").and_then(|a| a.as_str()) {
            Some(a) => a.to_string(),
            None => {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some("Missing action parameter".into()),
                });
            }
        };

        // Launch Browser with standard headless args and no-sandbox to ensure execution safety inside containers
        let config = BrowserConfig::builder()
            .arg("--no-sandbox")
            .arg("--disable-gpu")
            .build()
            .map_err(|e| anyhow::Error::msg(format!("Failed to build browser config: {e}")))?;

        let (mut browser, mut handler) = Browser::launch(config)
            .await
            .map_err(|e| anyhow::Error::msg(format!("Failed to launch browser: {e}")))?;

        // Spawn handler task
        let handler_handle = tokio::spawn(async move { while handler.next().await.is_some() {} });

        let res = async {
            let page = browser
                .new_page("about:blank")
                .await?;

            match action.as_str() {
                "navigate" => {
                    let target_url = args
                        .get("url")
                        .and_then(|u| u.as_str())
                        .ok_or_else(|| anyhow::Error::msg("Missing url parameter"))?;
                    page.goto(target_url).await?;
                    page.wait_for_navigation().await?;
                    let title = page.get_title().await?.unwrap_or_default();
                    let final_url = page.url().await?.unwrap_or_default();
                    Ok(format!("Successfully navigated to: {final_url} (Title: '{title}')"))
                }
                "screenshot" => {
                    if let Some(target_url) = args.get("url").and_then(|u| u.as_str()) {
                        page.goto(target_url).await?;
                        page.wait_for_navigation().await?;
                    }
                    let filename = args
                        .get("filename")
                        .and_then(|v| v.as_str())
                        .unwrap_or("cdp_screenshot.png");

                    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
                    let safe_name = PathBuf::from(&filename).file_name().map_or_else(
                        || format!("cdp_screenshot_{timestamp}.png"),
                        |n| n.to_string_lossy().to_string(),
                    );

                    let output_path = self.security.workspace_dir.join(&safe_name);
                    let screenshot_data = page
                        .screenshot(CaptureScreenshotParams::default())
                        .await?;

                    tokio::fs::write(&output_path, &screenshot_data).await?;
                    Ok(format!("Saved screenshot to {}", output_path.display()))
                }
                "click" => {
                    if let Some(target_url) = args.get("url").and_then(|u| u.as_str()) {
                        page.goto(target_url).await?;
                        page.wait_for_navigation().await?;
                    }
                    let selector = args
                        .get("selector")
                        .and_then(|s| s.as_str())
                        .ok_or_else(|| anyhow::Error::msg("Missing selector parameter"))?;

                    page.find_element(selector).await?.click().await?;
                    Ok(format!("Clicked element matching selector '{selector}'"))
                }
                "type" => {
                    if let Some(target_url) = args.get("url").and_then(|u| u.as_str()) {
                        page.goto(target_url).await?;
                        page.wait_for_navigation().await?;
                    }
                    let selector = args
                        .get("selector")
                        .and_then(|s| s.as_str())
                        .ok_or_else(|| anyhow::Error::msg("Missing selector parameter"))?;
                    let text = args
                        .get("text")
                        .and_then(|t| t.as_str())
                        .ok_or_else(|| anyhow::Error::msg("Missing text parameter"))?;

                    let element = page.find_element(selector).await?;
                    element.click().await?;
                    element.type_str(text).await?;
                    Ok(format!("Typed '{text}' into selector '{selector}'"))
                }
                "evaluate" => {
                    if let Some(target_url) = args.get("url").and_then(|u| u.as_str()) {
                        page.goto(target_url).await?;
                        page.wait_for_navigation().await?;
                    }
                    let script = args
                        .get("text")
                        .and_then(|t| t.as_str())
                        .ok_or_else(|| anyhow::Error::msg("Missing text parameter representing Javascript script"))?;

                    let execution_result = page.evaluate(script).await?;
                    let value = execution_result.into_value::<serde_json::Value>()?;
                    Ok(format!("Script output: {}", serde_json::to_string_pretty(&value)?))
                }
                "interactive_elements" => {
                    if let Some(target_url) = args.get("url").and_then(|u| u.as_str()) {
                        page.goto(target_url).await?;
                        page.wait_for_navigation().await?;
                    }
                    let js = r#"
                        () => {
                            const items = [];
                            const elements = document.querySelectorAll('button, a, input, textarea, select, [role="button"], [onclick]');
                            for (let i = 0; i < elements.length; i++) {
                                const el = elements[i];
                                const rect = el.getBoundingClientRect();
                                if (rect.width > 0 && rect.height > 0 && window.getComputedStyle(el).visibility !== 'hidden') {
                                    let selector = el.tagName.toLowerCase();
                                    if (el.id) {
                                        selector += `#${el.id}`;
                                    } else if (el.className) {
                                        selector += `.${el.className.trim().split(/\s+/).join('.')}`;
                                    }
                                    items.push({
                                        index: i + 1,
                                        tag: el.tagName,
                                        text: el.innerText || el.value || el.placeholder || '',
                                        x: Math.round(rect.left + rect.width / 2),
                                        y: Math.round(rect.top + rect.height / 2),
                                        width: Math.round(rect.width),
                                        height: Math.round(rect.height),
                                        selector: selector
                                    });
                                }
                            }
                            return items;
                        }
                    "#;
                    let execution_result = page.evaluate(js).await?;
                    let value = execution_result.into_value::<serde_json::Value>()?;
                    Ok(serde_json::to_string_pretty(&value)?)
                }
                "visual_targets" => {
                    if let Some(target_url) = args.get("url").and_then(|u| u.as_str()) {
                        page.goto(target_url).await?;
                        page.wait_for_navigation().await?;
                    }

                    let inject_js = r#"
                        () => {
                            const elements = document.querySelectorAll('button, a, input, textarea, select, [role="button"], [onclick]');
                            let count = 0;
                            for (let i = 0; i < elements.length; i++) {
                                const el = elements[i];
                                const rect = el.getBoundingClientRect();
                                if (rect.width > 0 && rect.height > 0 && window.getComputedStyle(el).visibility !== 'hidden') {
                                    count++;
                                    const marker = document.createElement('div');
                                    marker.className = 'zeroclaw-marker';
                                    marker.style.position = 'absolute';
                                    marker.style.left = (window.scrollX + rect.left) + 'px';
                                    marker.style.top = (window.scrollY + rect.top) + 'px';
                                    marker.style.width = rect.width + 'px';
                                    marker.style.height = rect.height + 'px';
                                    marker.style.border = '2px solid red';
                                    marker.style.boxSizing = 'border-box';
                                    marker.style.zIndex = '100000';
                                    marker.style.pointerEvents = 'none';
                                    
                                    const label = document.createElement('span');
                                    label.innerText = count.toString();
                                    label.style.position = 'absolute';
                                    label.style.top = '0';
                                    label.style.left = '0';
                                    label.style.backgroundColor = 'red';
                                    label.style.color = 'white';
                                    label.style.fontWeight = 'bold';
                                    label.style.fontSize = '12px';
                                    label.style.padding = '2px';
                                    label.style.borderRadius = '2px';
                                    
                                    marker.appendChild(label);
                                    document.body.appendChild(marker);
                                }
                            }
                        }
                    "#;
                    page.evaluate(inject_js).await?;

                    let filename = args
                        .get("filename")
                        .and_then(|v| v.as_str())
                        .unwrap_or("cdp_visual_targets.png");

                    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
                    let safe_name = PathBuf::from(&filename).file_name().map_or_else(
                        || format!("cdp_visual_targets_{timestamp}.png"),
                        |n| n.to_string_lossy().to_string(),
                    );

                    let output_path = self.security.workspace_dir.join(&safe_name);
                    let screenshot_data = page
                        .screenshot(CaptureScreenshotParams::default())
                        .await?;

                    tokio::fs::write(&output_path, &screenshot_data).await?;

                    let cleanup_js = r#"
                        () => {
                            const markers = document.querySelectorAll('.zeroclaw-marker');
                            markers.forEach(m => m.remove());
                        }
                    "#;
                    let _ = page.evaluate(cleanup_js).await;

                    Ok(format!("Saved labeled visual targets screenshot to {}", output_path.display()))
                }
                _ => Err(anyhow::Error::msg(format!("Unsupported action: {action}"))),
            }
        }
        .await;

        // Cleanup browser
        let _ = browser.close().await;
        handler_handle.abort();

        match res {
            Ok(output) => Ok(ToolResult {
                success: true,
                output,
                error: None,
            }),
            Err(e) => Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(e.to_string()),
            }),
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
    fn cdp_browser_tool_metadata() {
        let tool = CdpBrowserTool::new(test_security());
        assert_eq!(tool.name(), "cdp_browser");
        assert!(tool.description().contains("CDP"));
    }
}
