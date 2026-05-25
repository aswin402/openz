//! Tool execution helpers extracted from `loop_`.
//!
//! Contains the functions responsible for invoking tools (single, parallel,
//! sequential) and the decision logic for choosing between parallel and
//! sequential execution.

use anyhow::Result;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

use crate::approval::ApprovalManager;
use crate::observability::{Observer, ObserverEvent};
use crate::tools::Tool;

// Items that still live in `loop_` — import via the parent module.
use super::loop_::{ParsedToolCall, ToolLoopCancelled, scrub_credentials};

// ── Helpers ──────────────────────────────────────────────────────────────

/// Look up a tool by name in a slice of boxed `dyn Tool` values.
pub fn find_tool<'a>(tools: &'a [Box<dyn Tool>], name: &str) -> Option<&'a dyn Tool> {
    tools.iter().find(|t| t.name() == name).map(|t| t.as_ref())
}

// ── Outcome ──────────────────────────────────────────────────────────────

pub struct ToolExecutionOutcome {
    pub output: String,
    pub success: bool,
    pub error_reason: Option<String>,
    pub duration: Duration,
    /// Cryptographic HMAC receipt proving this tool actually executed.
    /// Present only when tool receipts are enabled in config.
    pub receipt: Option<String>,
}

// ── Single tool execution ────────────────────────────────────────────────

pub async fn execute_one_tool(
    call_name: &str,
    call_arguments: serde_json::Value,
    tool_call_id: Option<&str>,
    tools_registry: &[Box<dyn Tool>],
    activated_tools: Option<&std::sync::Arc<std::sync::Mutex<crate::tools::ActivatedToolSet>>>,
    observer: &dyn Observer,
    cancellation_token: Option<&CancellationToken>,
    receipt_generator: Option<&super::tool_receipts::ReceiptGenerator>,
) -> Result<ToolExecutionOutcome> {
    crate::agent::loop_::emit_tui_event(crate::agent::tui_events::RuntimeEvent::ToolStarted(
        call_name.to_string(),
    ));

    let res = execute_one_tool_inner(
        call_name,
        call_arguments,
        tool_call_id,
        tools_registry,
        activated_tools,
        observer,
        cancellation_token,
        receipt_generator,
    )
    .await;

    let success = match &res {
        Ok(outcome) => outcome.success,
        Err(_) => false,
    };
    crate::agent::loop_::emit_tui_event(crate::agent::tui_events::RuntimeEvent::ToolFinished {
        name: call_name.to_string(),
        success,
    });

    res
}

async fn execute_one_tool_inner(
    call_name: &str,
    call_arguments: serde_json::Value,
    tool_call_id: Option<&str>,
    tools_registry: &[Box<dyn Tool>],
    activated_tools: Option<&std::sync::Arc<std::sync::Mutex<crate::tools::ActivatedToolSet>>>,
    observer: &dyn Observer,
    cancellation_token: Option<&CancellationToken>,
    receipt_generator: Option<&super::tool_receipts::ReceiptGenerator>,
) -> Result<ToolExecutionOutcome> {
    // Serialize arguments once and carry the full JSON into both observer
    // events. Previously the start event received a 300-char summary and the
    // completion event received no arguments at all, which made tool spans
    // opaque in OTel backends (see upstream issue #5980 — "Otel Traces Should
    // Include More Details About Why A Tool Call Failed"). Size is bounded
    // downstream by the tracing exporter, so we don't need to clip here.
    let full_args = call_arguments.to_string();
    let tool_call_id_owned = tool_call_id.map(str::to_string);
    observer.record_event(&ObserverEvent::ToolCallStart {
        tool: call_name.to_string(),
        tool_call_id: tool_call_id_owned.clone(),
        arguments: Some(full_args.clone()),
    });
    let start = Instant::now();

    let static_tool = find_tool(tools_registry, call_name);
    let activated_arc = if static_tool.is_none() {
        activated_tools.and_then(|at| at.lock().unwrap().get_resolved(call_name))
    } else {
        None
    };
    let Some(tool) = static_tool.or(activated_arc.as_deref()) else {
        let reason = format!("Unknown tool: {call_name}");
        let duration = start.elapsed();
        let scrubbed_reason = scrub_credentials(&reason);
        observer.record_event(&ObserverEvent::ToolCall {
            tool: call_name.to_string(),
            tool_call_id: tool_call_id_owned.clone(),
            duration,
            success: false,
            arguments: Some(full_args.clone()),
            result: Some(scrubbed_reason.clone()),
        });
        return Ok(ToolExecutionOutcome {
            output: reason,
            success: false,
            error_reason: Some(scrubbed_reason),
            duration,
            receipt: None,
        });
    };

    use ::zeroclaw_log::Instrument;
    let tool_span = ::zeroclaw_log::info_span!(
        target: "zeroclaw_log_internal_scope",
        "zeroclaw_scope",
        tool = %call_name,
    );

    // Auto tool I/O propagation: emit Start with full input, run the
    // tool, then emit Complete or Fail with full output. Per-tool
    // execute() impls add zero logging.
    let _start_guard = tool_span.clone().entered();
    ::zeroclaw_log::record!(
        INFO,
        ::zeroclaw_log::Event::new(module_path!(), ::zeroclaw_log::Action::Invoke)
            .with_category(::zeroclaw_log::EventCategory::Tool)
            .with_attrs(::serde_json::json!({"input": call_arguments})),
        "tool invocation start"
    );
    drop(_start_guard);

    let tool_future = tool
        .execute(call_arguments.clone())
        .instrument(tool_span.clone());
    let mut tool_result = if let Some(token) = cancellation_token {
        tokio::select! {
            () = token.cancelled() => return Err(ToolLoopCancelled.into()),
            result = tool_future => result,
        }
    } else {
        tool_future.await
    };

    // If the native tool execution failed, automatically try to execute the duplicate MCP fallback tool
    let is_failure = match &tool_result {
        Ok(r) => !r.success,
        Err(_) => true,
    };
    if is_failure {
        if let Some(mcp_tool_name) =
            find_fallback_tool_name(call_name, tools_registry, activated_tools)
        {
            ::zeroclaw_log::record!(
                INFO,
                ::zeroclaw_log::Event::new(module_path!(), ::zeroclaw_log::Action::Invoke)
                    .with_category(::zeroclaw_log::EventCategory::Tool),
                &format!(
                    "Native tool '{}' failed. Automatically falling back to MCP tool '{}'.",
                    call_name, mcp_tool_name
                )
            );
            let translated_args =
                translate_arguments(call_name, &mcp_tool_name, call_arguments.clone());

            // Try to find the fallback tool instance
            let mut fallback_tool = None;

            // 1. Try to find/activate from deferred set first via ToolSearchTool
            if let Some(tool_search) = find_tool(tools_registry, "tool_search") {
                if let Some(tool_search_tool) = tool_search
                    .as_any()
                    .and_then(|any| any.downcast_ref::<crate::tools::ToolSearchTool>())
                {
                    if let Some(act_tool) =
                        tool_search_tool.activate_deferred_mcp_tool(&mcp_tool_name)
                    {
                        fallback_tool = Some(act_tool);
                    }
                }
            }

            // 2. Try to find in activated_tools
            if fallback_tool.is_none() {
                if let Some(at) = activated_tools {
                    fallback_tool = at.lock().unwrap().get(&mcp_tool_name);
                }
            }

            // Execute the fallback
            let fallback_result = if let Some(ref f_tool) = fallback_tool {
                let f_future = f_tool
                    .execute(translated_args)
                    .instrument(tool_span.clone());
                if let Some(token) = cancellation_token {
                    tokio::select! {
                        () = token.cancelled() => return Err(ToolLoopCancelled.into()),
                        res = f_future => res,
                    }
                } else {
                    f_future.await
                }
            } else if let Some(f_tool) = find_tool(tools_registry, &mcp_tool_name) {
                let f_future = f_tool
                    .execute(translated_args)
                    .instrument(tool_span.clone());
                if let Some(token) = cancellation_token {
                    tokio::select! {
                        () = token.cancelled() => return Err(ToolLoopCancelled.into()),
                        res = f_future => res,
                    }
                } else {
                    f_future.await
                }
            } else {
                Err(anyhow::anyhow!(
                    "Fallback MCP tool '{}' found but failed to load",
                    mcp_tool_name
                ))
            };

            tool_result = fallback_result;
        }
    }

    let _result_guard = tool_span.entered();
    match tool_result {
        Ok(r) => {
            let duration = start.elapsed();
            if r.success {
                ::zeroclaw_log::record!(
                    INFO,
                    ::zeroclaw_log::Event::new(module_path!(), ::zeroclaw_log::Action::Complete)
                        .with_category(::zeroclaw_log::EventCategory::Tool)
                        .with_outcome(::zeroclaw_log::EventOutcome::Success)
                        .with_duration(duration.as_millis() as u64)
                        .with_attrs(::serde_json::json!({"output": r.output})),
                    "tool invocation complete"
                );
            } else {
                ::zeroclaw_log::record!(WARN, ::zeroclaw_log::Event::new(module_path!(), ::zeroclaw_log::Action::Fail).with_category(::zeroclaw_log::EventCategory::Tool).with_outcome(::zeroclaw_log::EventOutcome::Failure).with_duration(duration.as_millis() as u64).with_attrs(::serde_json::json!({"error": r.error.clone().unwrap_or_default(), "output": r.output})), "tool invocation failed");
            }
            if r.success {
                let normalized_output = if r.output.is_empty() {
                    "(no output)"
                } else {
                    &r.output
                };
                let output = scrub_credentials(normalized_output);
                let receipt = receipt_generator.map(|receipt_gen| {
                    receipt_gen.generate_now(call_name, &call_arguments, &output)
                });
                observer.record_event(&ObserverEvent::ToolCall {
                    tool: call_name.to_string(),
                    tool_call_id: tool_call_id_owned.clone(),
                    duration,
                    success: true,
                    arguments: Some(full_args.clone()),
                    result: Some(output.clone()),
                });
                Ok(ToolExecutionOutcome {
                    output,
                    success: true,
                    error_reason: None,
                    duration,
                    receipt,
                })
            } else {
                let reason = r.error.unwrap_or(r.output);
                let scrubbed_reason = scrub_credentials(&reason);
                observer.record_event(&ObserverEvent::ToolCall {
                    tool: call_name.to_string(),
                    tool_call_id: tool_call_id_owned.clone(),
                    duration,
                    success: false,
                    arguments: Some(full_args.clone()),
                    result: Some(scrubbed_reason.clone()),
                });
                Ok(ToolExecutionOutcome {
                    output: format!("Error: {reason}"),
                    success: false,
                    error_reason: Some(scrubbed_reason),
                    duration,
                    receipt: None,
                })
            }
        }
        Err(e) => {
            let duration = start.elapsed();
            ::zeroclaw_log::record!(
                ERROR,
                ::zeroclaw_log::Event::new(module_path!(), ::zeroclaw_log::Action::Fail)
                    .with_category(::zeroclaw_log::EventCategory::Tool)
                    .with_outcome(::zeroclaw_log::EventOutcome::Failure)
                    .with_duration(duration.as_millis() as u64)
                    .with_attrs(::serde_json::json!({"error": format!("{e:?}")})),
                "tool invocation errored"
            );
            let reason = format!("Error executing {call_name}: {e}");
            let scrubbed_reason = scrub_credentials(&reason);
            observer.record_event(&ObserverEvent::ToolCall {
                tool: call_name.to_string(),
                tool_call_id: tool_call_id_owned.clone(),
                duration,
                success: false,
                arguments: Some(full_args.clone()),
                result: Some(scrubbed_reason.clone()),
            });
            Ok(ToolExecutionOutcome {
                output: reason,
                success: false,
                error_reason: Some(scrubbed_reason),
                duration,
                receipt: None,
            })
        }
    }
}

// ── Parallel / sequential decision ───────────────────────────────────────

pub fn should_execute_tools_in_parallel(
    tool_calls: &[ParsedToolCall],
    approval: Option<&ApprovalManager>,
) -> bool {
    if tool_calls.len() <= 1 {
        return false;
    }

    // tool_search activates deferred MCP tools into ActivatedToolSet.
    // Running tool_search in parallel with the tools it activates causes a
    // race condition where the tool lookup happens before activation completes.
    // Force sequential execution whenever tool_search is in the batch.
    if tool_calls.iter().any(|call| call.name == "tool_search") {
        return false;
    }

    if let Some(mgr) = approval
        && tool_calls.iter().any(|call| mgr.needs_approval(&call.name))
    {
        // Approval-gated calls must keep sequential handling so the caller can
        // enforce CLI prompt/deny policy consistently.
        return false;
    }

    true
}

// ── Parallel execution ───────────────────────────────────────────────────

pub async fn execute_tools_parallel(
    tool_calls: &[ParsedToolCall],
    tools_registry: &[Box<dyn Tool>],
    activated_tools: Option<&std::sync::Arc<std::sync::Mutex<crate::tools::ActivatedToolSet>>>,
    observer: &dyn Observer,
    cancellation_token: Option<&CancellationToken>,
    receipt_generator: Option<&super::tool_receipts::ReceiptGenerator>,
) -> Result<Vec<ToolExecutionOutcome>> {
    let futures: Vec<_> = tool_calls
        .iter()
        .map(|call| {
            execute_one_tool(
                &call.name,
                call.arguments.clone(),
                call.tool_call_id.as_deref(),
                tools_registry,
                activated_tools,
                observer,
                cancellation_token,
                receipt_generator,
            )
        })
        .collect();

    let results = futures_util::future::join_all(futures).await;
    results.into_iter().collect()
}

// ── Sequential execution ─────────────────────────────────────────────────

pub async fn execute_tools_sequential(
    tool_calls: &[ParsedToolCall],
    tools_registry: &[Box<dyn Tool>],
    activated_tools: Option<&std::sync::Arc<std::sync::Mutex<crate::tools::ActivatedToolSet>>>,
    observer: &dyn Observer,
    cancellation_token: Option<&CancellationToken>,
    receipt_generator: Option<&super::tool_receipts::ReceiptGenerator>,
) -> Result<Vec<ToolExecutionOutcome>> {
    let mut outcomes = Vec::with_capacity(tool_calls.len());

    for call in tool_calls {
        outcomes.push(
            execute_one_tool(
                &call.name,
                call.arguments.clone(),
                call.tool_call_id.as_deref(),
                tools_registry,
                activated_tools,
                observer,
                cancellation_token,
                receipt_generator,
            )
            .await?,
        );
    }

    Ok(outcomes)
}

fn find_fallback_tool_name(
    native_tool_name: &str,
    tools_registry: &[Box<dyn Tool>],
    activated_tools: Option<&std::sync::Arc<std::sync::Mutex<crate::tools::ActivatedToolSet>>>,
) -> Option<String> {
    // List of (target_name, is_exact) to try
    let fallbacks = match native_tool_name {
        "file_read" => vec![("filesystem__read_file", true)],
        "file_write" => vec![("filesystem__write_file", true)],
        "file_edit" => vec![("filesystem__edit_file", true)],
        "glob_search" => vec![("fd__", false), ("ripgrep__", false)],
        "codebase_snapshot" => vec![("repomix__", false)],
        "git_native" | "git_operations" => vec![("git__", false)],
        "browser" | "cdp_browser" => vec![("playwright__", false), ("puppeteer__", false)],
        "web_fetch" | "web_parse" => vec![("fetch__", false), ("firecrawl__", false)],
        "web_search_tool" | "web_search" => vec![
            ("tavily__", false),
            ("duckduckgo__", false),
            ("brave-search__", false),
        ],
        _ => return None,
    };

    let check_match = |name: &str| -> bool {
        for (target, is_exact) in &fallbacks {
            if *is_exact {
                if name == *target {
                    return true;
                }
            } else {
                if name.starts_with(*target) {
                    return true;
                }
            }
        }
        false
    };

    // 1. Check activated_tools first
    if let Some(at) = activated_tools {
        let guard = at.lock().unwrap();
        for name in guard.tool_names() {
            if check_match(name) {
                return Some(name.to_string());
            }
        }
    }

    // 2. Check tools_registry next
    for tool in tools_registry {
        let name = tool.name();
        if check_match(name) {
            return Some(name.to_string());
        }
    }

    // 3. Check stubs in ToolSearchTool
    if let Some(tool_search) = find_tool(tools_registry, "tool_search") {
        if let Some(tool_search_tool) = tool_search
            .as_any()
            .and_then(|any| any.downcast_ref::<crate::tools::ToolSearchTool>())
        {
            for stub in &tool_search_tool.deferred.stubs {
                if check_match(&stub.prefixed_name) {
                    return Some(stub.prefixed_name.clone());
                }
            }
        }
    }

    None
}

fn translate_arguments(
    native_tool: &str,
    mcp_tool: &str,
    args: serde_json::Value,
) -> serde_json::Value {
    match (native_tool, mcp_tool) {
        ("file_read", _) => {
            if let Some(path) = args.get("path") {
                serde_json::json!({ "path": path.clone() })
            } else {
                args
            }
        }
        ("file_write", _) => {
            if let Some(path) = args.get("path") {
                if let Some(content) = args.get("content") {
                    serde_json::json!({
                        "path": path.clone(),
                        "content": content.clone()
                    })
                } else {
                    args
                }
            } else {
                args
            }
        }
        ("glob_search", _) => {
            if let Some(pattern) = args.get("pattern") {
                serde_json::json!({
                    "pattern": pattern.clone(),
                    "glob": pattern.clone(),
                    "query": pattern.clone()
                })
            } else {
                args
            }
        }
        _ => args,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use zeroclaw_api::tool::ToolResult;

    struct MockTool {
        name: String,
        success: bool,
        output: String,
    }

    impl ::zeroclaw_api::attribution::Attributable for MockTool {
        fn role(&self) -> ::zeroclaw_api::attribution::Role {
            ::zeroclaw_api::attribution::Role::Tool(::zeroclaw_api::attribution::ToolKind::Plugin)
        }
        fn alias(&self) -> &str {
            &self.name
        }
    }

    #[async_trait::async_trait]
    impl Tool for MockTool {
        fn name(&self) -> &str {
            &self.name
        }
        fn description(&self) -> &str {
            "mock tool"
        }
        fn parameters_schema(&self) -> serde_json::Value {
            json!({})
        }
        async fn execute(&self, _args: serde_json::Value) -> anyhow::Result<ToolResult> {
            if self.success {
                Ok(ToolResult {
                    success: true,
                    output: self.output.clone(),
                    error: None,
                })
            } else {
                Ok(ToolResult {
                    success: false,
                    output: "Native failed".to_string(),
                    error: Some("Error executing".to_string()),
                })
            }
        }
    }

    #[tokio::test]
    async fn test_tool_fallback_on_failure() {
        let native = MockTool {
            name: "file_read".to_string(),
            success: false,
            output: "".to_string(),
        };
        let fallback = MockTool {
            name: "filesystem__read_file".to_string(),
            success: true,
            output: "Fallback content".to_string(),
        };

        let tools_registry: Vec<Box<dyn Tool>> = vec![Box::new(native), Box::new(fallback)];

        let observer = crate::observability::NoopObserver;

        let result = execute_one_tool(
            "file_read",
            json!({"path": "test.txt"}),
            None,
            &tools_registry,
            None,
            &observer,
            None,
            None,
        )
        .await
        .unwrap();

        assert!(result.success);
        assert_eq!(result.output, "Fallback content");
    }
}
