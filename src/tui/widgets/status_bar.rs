use crate::tui::state::{
    ActivityState, LspStatus, RuntimeStatus, format_cost, format_git, format_tokens,
};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

pub fn draw_status_bar(f: &mut Frame, area: Rect, status: &RuntimeStatus, spinner_frame: &str) {
    let mut spans = Vec::new();

    // 1. Spinner & Activity
    match &status.current_activity {
        ActivityState::Idle => {
            spans.push(Span::styled("✓ Ready", Style::default().fg(Color::Green)));
        }
        ActivityState::Thinking => {
            spans.push(Span::styled(
                format!("{spinner_frame} Thinking..."),
                Style::default().fg(Color::Cyan),
            ));
        }
        ActivityState::CallingModel => {
            spans.push(Span::styled(
                format!("{spinner_frame} Calling model..."),
                Style::default().fg(Color::Cyan),
            ));
        }
        ActivityState::RunningTool(name) => {
            spans.push(Span::styled(
                format!("{spinner_frame} Running tool: {name}"),
                Style::default().fg(Color::Yellow),
            ));
        }
        ActivityState::RunningMcp(name) => {
            spans.push(Span::styled(
                format!("{spinner_frame} Running MCP tool: {name}"),
                Style::default().fg(Color::Yellow),
            ));
        }
        ActivityState::IndexingProject => {
            spans.push(Span::styled(
                format!("{spinner_frame} Indexing project..."),
                Style::default().fg(Color::Blue),
            ));
        }
        ActivityState::WritingFiles => {
            spans.push(Span::styled(
                format!("{spinner_frame} Writing files..."),
                Style::default().fg(Color::Blue),
            ));
        }
        ActivityState::WaitingForResponse => {
            spans.push(Span::styled(
                format!("{spinner_frame} Waiting for response..."),
                Style::default().fg(Color::Cyan),
            ));
        }
        ActivityState::Error(err) => {
            spans.push(Span::styled(
                format!("✗ Error: {err}"),
                Style::default().fg(Color::Red),
            ));
        }
    }

    // Divider
    spans.push(Span::raw(" · "));

    // 2. Model
    let model = status.active_model.as_deref().unwrap_or("none");
    spans.push(Span::styled(
        model,
        Style::default()
            .fg(Color::Magenta)
            .add_modifier(Modifier::BOLD),
    ));

    // Divider
    spans.push(Span::raw(" · "));

    // 3. Tokens
    let total_tokens = status.prompt_tokens + status.completion_tokens;
    spans.push(Span::raw(format_tokens(Some(total_tokens))));

    // Divider
    spans.push(Span::raw(" · "));

    // 4. Cost
    spans.push(Span::styled(
        format_cost(status.estimated_cost_usd),
        Style::default().fg(Color::Green),
    ));

    // Divider
    spans.push(Span::raw(" · "));

    // 5. MCP count
    let mcp_count = status.mcp_servers.len();
    spans.push(Span::raw(format!("MCP {mcp_count}")));

    // Divider
    spans.push(Span::raw(" · "));

    // 6. LSP Status
    let lsp_str = match status.lsp_status {
        LspStatus::Active => "LSP active",
        LspStatus::Inactive => "LSP inactive",
    };
    let lsp_style = match status.lsp_status {
        LspStatus::Active => Style::default().fg(Color::Green),
        LspStatus::Inactive => Style::default().fg(Color::DarkGray),
    };
    spans.push(Span::styled(lsp_str, lsp_style));

    // Divider
    spans.push(Span::raw(" · "));

    // 7. Git info
    spans.push(Span::styled(
        format_git(status.git_branch.as_deref(), status.git_dirty),
        Style::default().fg(Color::LightBlue),
    ));

    // Render paragraph
    let line = Line::from(spans);
    let p = Paragraph::new(line).style(Style::default().bg(Color::Rgb(30, 30, 30)));
    f.render_widget(p, area);
}
