use async_trait::async_trait;
use enigo::{
    Button, Coordinate,
    Direction::{Click, Press, Release},
    Enigo, Key, Keyboard, Mouse, Settings,
};
use serde_json::json;
use std::sync::Arc;
use zeroclaw_api::tool::{Tool, ToolResult};
use zeroclaw_config::policy::SecurityPolicy;

/// Tool for simulating OS-level keyboard and mouse inputs.
pub struct OSInputTool {
    security: Arc<SecurityPolicy>,
}

impl OSInputTool {
    pub fn new(security: Arc<SecurityPolicy>) -> Self {
        Self { security }
    }
}

fn parse_key(key_str: &str) -> Option<Key> {
    match key_str.to_lowercase().as_str() {
        "alt" => Some(Key::Alt),
        "command" | "cmd" | "super" | "win" | "windows" => Some(Key::Meta),
        "control" | "ctrl" => Some(Key::Control),
        "shift" => Some(Key::Shift),
        "backspace" => Some(Key::Backspace),
        "delete" | "del" => Some(Key::Delete),
        "down" | "downarrow" => Some(Key::DownArrow),
        "end" => Some(Key::End),
        "escape" | "esc" => Some(Key::Escape),
        "home" => Some(Key::Home),
        "left" | "leftarrow" => Some(Key::LeftArrow),
        "pagedown" | "pgdn" => Some(Key::PageDown),
        "pageup" | "pgup" => Some(Key::PageUp),
        "return" | "enter" => Some(Key::Return),
        "right" | "rightarrow" => Some(Key::RightArrow),
        "space" => Some(Key::Space),
        "tab" => Some(Key::Tab),
        "up" | "uparrow" => Some(Key::UpArrow),
        s if s.len() == 1 => Some(Key::Unicode(s.chars().next().unwrap())),
        _ => None,
    }
}

#[async_trait]
impl Tool for OSInputTool {
    fn name(&self) -> &str {
        "os_input"
    }

    fn description(&self) -> &str {
        "Simulate OS-level mouse clicks, movement, dragging, key presses, and typing text."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["click", "move", "type", "press_key", "release_key", "drag"],
                    "description": "The input simulation action to perform"
                },
                "x": {
                    "type": "integer",
                    "description": "X coordinate for click, move, drag actions"
                },
                "y": {
                    "type": "integer",
                    "description": "Y coordinate for click, move, drag actions"
                },
                "text": {
                    "type": "string",
                    "description": "Text to simulate typing"
                },
                "key": {
                    "type": "string",
                    "description": "Special key name (e.g. 'enter', 'ctrl', 'shift', 'tab', or a single character)"
                },
                "button": {
                    "type": "string",
                    "enum": ["left", "right", "middle"],
                    "description": "Mouse button to click (default is left)"
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

        let res = tokio::task::spawn_blocking(move || -> anyhow::Result<String> {
            let mut enigo = Enigo::new(&Settings::default())
                .map_err(|e| anyhow::Error::msg(format!("Failed to initialize Enigo: {e}")))?;

            match action.as_str() {
                "move" => {
                    let x = args
                        .get("x")
                        .and_then(|v| v.as_i64())
                        .ok_or_else(|| anyhow::Error::msg("Missing x coordinate"))?
                        as i32;
                    let y = args
                        .get("y")
                        .and_then(|v| v.as_i64())
                        .ok_or_else(|| anyhow::Error::msg("Missing y coordinate"))?
                        as i32;
                    enigo
                        .move_mouse(x, y, Coordinate::Abs)
                        .map_err(|e| anyhow::Error::msg(format!("Mouse move failed: {e}")))?;
                    Ok(format!("Moved mouse to ({x}, {y})"))
                }
                "click" => {
                    if let (Some(x), Some(y)) = (
                        args.get("x").and_then(|v| v.as_i64()),
                        args.get("y").and_then(|v| v.as_i64()),
                    ) {
                        enigo
                            .move_mouse(x as i32, y as i32, Coordinate::Abs)
                            .map_err(|e| anyhow::Error::msg(format!("Mouse move failed: {e}")))?;
                    }
                    let btn_str = args
                        .get("button")
                        .and_then(|v| v.as_str())
                        .unwrap_or("left");
                    let btn = match btn_str {
                        "right" => Button::Right,
                        "middle" => Button::Middle,
                        _ => Button::Left,
                    };
                    enigo
                        .button(btn, Click)
                        .map_err(|e| anyhow::Error::msg(format!("Mouse click failed: {e}")))?;
                    Ok(format!("Clicked mouse button '{btn_str}'"))
                }
                "drag" => {
                    let x = args
                        .get("x")
                        .and_then(|v| v.as_i64())
                        .ok_or_else(|| anyhow::Error::msg("Missing x coordinate"))?
                        as i32;
                    let y = args
                        .get("y")
                        .and_then(|v| v.as_i64())
                        .ok_or_else(|| anyhow::Error::msg("Missing y coordinate"))?
                        as i32;
                    enigo
                        .button(Button::Left, Press)
                        .map_err(|e| anyhow::Error::msg(format!("Mouse press failed: {e}")))?;
                    enigo
                        .move_mouse(x, y, Coordinate::Abs)
                        .map_err(|e| anyhow::Error::msg(format!("Mouse move failed: {e}")))?;
                    enigo
                        .button(Button::Left, Release)
                        .map_err(|e| anyhow::Error::msg(format!("Mouse release failed: {e}")))?;
                    Ok(format!("Dragged mouse to ({x}, {y})"))
                }
                "type" => {
                    let text = args
                        .get("text")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| anyhow::Error::msg("Missing text to type"))?;
                    enigo
                        .text(text)
                        .map_err(|e| anyhow::Error::msg(format!("Text entry failed: {e}")))?;
                    Ok(format!("Typed text: {text}"))
                }
                "press_key" => {
                    let key_str = args
                        .get("key")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| anyhow::Error::msg("Missing key name"))?;
                    let key = parse_key(key_str).ok_or_else(|| {
                        anyhow::Error::msg(format!("Unknown/unsupported key: {key_str}"))
                    })?;
                    enigo
                        .key(key, Press)
                        .map_err(|e| anyhow::Error::msg(format!("Key press failed: {e}")))?;
                    Ok(format!("Pressed key: {key_str}"))
                }
                "release_key" => {
                    let key_str = args
                        .get("key")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| anyhow::Error::msg("Missing key name"))?;
                    let key = parse_key(key_str).ok_or_else(|| {
                        anyhow::Error::msg(format!("Unknown/unsupported key: {key_str}"))
                    })?;
                    enigo
                        .key(key, Release)
                        .map_err(|e| anyhow::Error::msg(format!("Key release failed: {e}")))?;
                    Ok(format!("Released key: {key_str}"))
                }
                _ => Err(anyhow::Error::msg(format!("Unsupported action: {action}"))),
            }
        })
        .await
        .map_err(|e| anyhow::Error::msg(format!("Execution thread panicked: {e}")))?;

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
    fn os_input_tool_metadata() {
        let tool = OSInputTool::new(test_security());
        assert_eq!(tool.name(), "os_input");
        assert!(tool.description().contains("OS-level"));
    }

    #[test]
    fn test_parse_key() {
        assert_eq!(parse_key("ctrl"), Some(Key::Control));
        assert_eq!(parse_key("a"), Some(Key::Unicode('a')));
        assert_eq!(parse_key("unknown_key_name"), None);
    }
}
