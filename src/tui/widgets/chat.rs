use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph, Wrap},
};
use zeroclaw_providers::ChatMessage;

pub fn draw_chat(f: &mut Frame, area: Rect, history: &[ChatMessage], scroll_offset: usize) {
    let mut lines = Vec::new();

    for msg in history {
        // Skip or format system prompt subtly
        if msg.role == "system" {
            lines.push(Line::from(vec![Span::styled(
                "System Prompt Active",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::ITALIC),
            )]));
            lines.push(Line::raw(""));
            continue;
        }

        let prefix = match msg.role.as_str() {
            "user" => Span::styled(
                "User: ",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            "assistant" => Span::styled(
                "Assistant: ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            "tool" => Span::styled(
                "Tool Result: ",
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::ITALIC),
            ),
            _ => Span::styled(
                format!("{}: ", msg.role),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::ITALIC),
            ),
        };

        lines.push(Line::from(vec![prefix, Span::raw(msg.content.trim())]));
        // Add empty line between messages
        lines.push(Line::raw(""));
    }

    let chat_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(" Chat History ")
        .border_style(Style::default().fg(Color::Blue));

    let chat_paragraph = Paragraph::new(lines)
        .block(chat_block)
        .wrap(Wrap { trim: true })
        .scroll((scroll_offset as u16, 0));

    f.render_widget(chat_paragraph, area);
}
