use ratatui::{
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame,
};

pub fn draw_input(
    f: &mut Frame,
    area: Rect,
    input_text: &str,
    is_thinking: bool,
) {
    let input_style = if is_thinking {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().fg(Color::Green)
    };

    let title = if is_thinking {
        " Agent is thinking (Esc to cancel) "
    } else {
        " Input Message (Enter to send, Shift+Enter for newline) "
    };

    let input_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(title)
        .border_style(input_style);

    let input_paragraph = Paragraph::new(input_text).block(input_block);

    f.render_widget(input_paragraph, area);

    // Position cursor at the end of the text
    if !is_thinking {
        let cursor_x = area.x + 1 + input_text.chars().count() as u16;
        let cursor_y = area.y + 1;
        f.set_cursor_position((cursor_x, cursor_y));
    }
}
