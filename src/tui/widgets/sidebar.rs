use crate::tui::state::{LspStatus, RuntimeStatus, format_git, truncate};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph, Wrap},
};

pub fn draw_sidebar(f: &mut Frame, area: Rect, status: &RuntimeStatus) {
    let mut lines = Vec::new();

    // 1. Session Section
    lines.push(Line::from(Span::styled(
        "🤖 AGENT SESSION",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(vec![
        Span::raw(" Provider: "),
        Span::styled(
            status.active_provider.as_deref().unwrap_or("none"),
            Style::default().add_modifier(Modifier::BOLD),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::raw(" Model:    "),
        Span::styled(
            status.active_model.as_deref().unwrap_or("none"),
            Style::default().add_modifier(Modifier::BOLD),
        ),
    ]));
    lines.push(Line::raw(""));

    // 2. Resources Section
    lines.push(Line::from(Span::styled(
        "💵 RESOURCES",
        Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD),
    )));
    let total_tokens = status.prompt_tokens + status.completion_tokens;
    lines.push(Line::from(format!(
        " Prompt:     {} tok",
        status.prompt_tokens
    )));
    lines.push(Line::from(format!(
        " Completion: {} tok",
        status.completion_tokens
    )));
    lines.push(Line::from(format!(" Total:      {} tok", total_tokens)));
    let cost_val = status.estimated_cost_usd.unwrap_or(0.0).max(0.0);
    lines.push(Line::from(format!(" Est. Cost:  ${:.4}", cost_val)));
    lines.push(Line::raw(""));

    // 3. Environment Section
    lines.push(Line::from(Span::styled(
        "💡 ENVIRONMENT",
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )));
    let lsp_val = match status.lsp_status {
        LspStatus::Active => Span::styled("Active", Style::default().fg(Color::Green)),
        LspStatus::Inactive => Span::styled("Inactive", Style::default().fg(Color::DarkGray)),
    };
    lines.push(Line::from(vec![Span::raw(" LSP Status: "), lsp_val]));
    lines.push(Line::from(format!(
        " Skills:     {} loaded",
        status.available_skills_count
    )));
    lines.push(Line::raw(""));

    // 4. Git Repository Section
    lines.push(Line::from(Span::styled(
        "🐙 GIT REPOSITORY",
        Style::default()
            .fg(Color::LightBlue)
            .add_modifier(Modifier::BOLD),
    )));
    let git_str = format_git(status.git_branch.as_deref(), status.git_dirty);
    lines.push(Line::from(vec![
        Span::raw(" Branch: "),
        Span::styled(git_str, Style::default().add_modifier(Modifier::BOLD)),
    ]));
    if !status.modified_files.is_empty() {
        lines.push(Line::from(vec![Span::raw(" Mod Files:")]));
        for file in status.modified_files.iter().take(4) {
            let filename = file.file_name().and_then(|s| s.to_str()).unwrap_or("file");
            lines.push(Line::from(vec![
                Span::raw("  • "),
                Span::styled(truncate(filename, 22), Style::default().fg(Color::DarkGray)),
            ]));
        }
        if status.modified_files.len() > 4 {
            lines.push(Line::from(vec![Span::raw(format!(
                "  • ... and {} more",
                status.modified_files.len() - 4
            ))]));
        }
    }
    lines.push(Line::raw(""));

    // 5. MCP Servers Section
    lines.push(Line::from(Span::styled(
        "🛠  MCP SERVERS",
        Style::default()
            .fg(Color::Magenta)
            .add_modifier(Modifier::BOLD),
    )));
    if status.mcp_servers.is_empty() {
        lines.push(Line::from(" none"));
    } else {
        for mcp in &status.mcp_servers {
            let badge = if mcp.connected {
                Span::styled(" ● ", Style::default().fg(Color::Green))
            } else {
                Span::styled(" ○ ", Style::default().fg(Color::Red))
            };
            lines.push(Line::from(vec![badge, Span::raw(truncate(&mcp.name, 22))]));
        }
    }

    // 6. Last Error (if present)
    if let Some(ref err) = status.last_error {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "✗ LAST ERROR",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(Span::styled(
            truncate(err, 32),
            Style::default().fg(Color::Red),
        )));
    }

    let sidebar_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(" Metadata ")
        .border_style(Style::default().fg(Color::Blue));

    let sidebar_paragraph = Paragraph::new(lines)
        .block(sidebar_block)
        .wrap(Wrap { trim: true });

    f.render_widget(sidebar_paragraph, area);
}
