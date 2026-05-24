use crate::agent::Agent;
use crate::agent::dispatcher::ParsedToolCall;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Option<serde_json::value::Value>,
    pub method: String,
    pub params: Option<Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Option<serde_json::value::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

pub async fn handle_mcp_request(
    agent: &Agent,
    request: JsonRpcRequest,
    bridge_deny_tools: &[String],
) -> Option<JsonRpcResponse> {
    let is_notification = request.id.is_none();
    let id = request.id.unwrap_or(Value::Null);

    let response = match request.method.as_str() {
        "initialize" => JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: Some(id),
            result: Some(serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {}
                },
                "serverInfo": {
                    "name": "zeroclaw-daemon",
                    "version": "0.8.0"
                }
            })),
            error: None,
        },
        "notifications/initialized" => {
            return None;
        }
        "tools/list" => {
            let tools: Vec<Value> = agent
                .tool_specs
                .iter()
                .map(|spec| {
                    serde_json::json!({
                        "name": spec.name,
                        "description": spec.description,
                        "inputSchema": spec.parameters
                    })
                })
                .collect();

            JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: Some(id),
                result: Some(serde_json::json!({
                    "tools": tools
                })),
                error: None,
            }
        }
        "tools/call" => {
            let params = match request.params {
                Some(p) => p,
                None => {
                    return Some(JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: Some(id),
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32602,
                            message: "Missing params".to_string(),
                            data: None,
                        }),
                    });
                }
            };

            let name = match params.get("name").and_then(Value::as_str) {
                Some(n) => n.to_string(),
                None => {
                    return Some(JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: Some(id),
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32602,
                            message: "Missing tool name".to_string(),
                            data: None,
                        }),
                    });
                }
            };

            let arguments = params
                .get("arguments")
                .cloned()
                .unwrap_or(Value::Object(serde_json::Map::new()));

            // Verify if tool is denied
            let is_denied = bridge_deny_tools.contains(&name);
            if is_denied {
                return Some(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: Some(id),
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32601,
                        message: format!("Tool is denied: {name}"),
                        data: None,
                    }),
                });
            }

            // Execute tool using Agent's execute_tool_call
            let call = ParsedToolCall {
                name,
                arguments,
                tool_call_id: None,
            };

            let result = agent.execute_tool_call(&call).await;

            let content = vec![serde_json::json!({
                "type": "text",
                "text": result.output
            })];

            JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: Some(id),
                result: Some(serde_json::json!({
                    "content": content,
                    "isError": !result.success
                })),
                error: None,
            }
        }
        _ => {
            if is_notification {
                return None;
            }
            JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: Some(id),
                result: None,
                error: Some(JsonRpcError {
                    code: -32601,
                    message: format!("Method not found: {}", request.method),
                    data: None,
                }),
            }
        }
    };

    if is_notification {
        None
    } else {
        Some(response)
    }
}
