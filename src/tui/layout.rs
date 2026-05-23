use ratatui::layout::{Constraint, Direction, Layout, Rect};

#[derive(Debug, Clone, Copy)]
pub struct TuiLayout {
    pub chat_area: Rect,
    pub status_bar_area: Rect,
    pub sidebar_area: Option<Rect>,
    pub input_area: Rect,
}

impl TuiLayout {
    pub fn select(width: u16, height: u16, show_sidebar: bool) -> Self {
        // Vertical layout splits:
        // 1. Top chunk (Chat + optional sidebar)
        // 2. Status bar (1 line)
        // 3. Input box (3 lines)
        let main_constraints = vec![
            Constraint::Min(0),      // Chat + Sidebar
            Constraint::Length(1),   // Status Bar
            Constraint::Length(3),   // Input
        ];

        let main_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(main_constraints)
            .split(Rect::new(0, 0, width, height));

        let top_area = main_chunks[0];
        let status_bar_area = main_chunks[1];
        let input_area = main_chunks[2];

        // Based on width and toggle state:
        if width >= 120 && show_sidebar {
            // Split top area horizontally into Chat (left) and Sidebar (right)
            let horizontal_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints(vec![
                    Constraint::Min(0),          // Chat
                    Constraint::Length(36),      // Fixed size sidebar
                ])
                .split(top_area);

            Self {
                chat_area: horizontal_chunks[0],
                status_bar_area,
                sidebar_area: Some(horizontal_chunks[1]),
                input_area,
            }
        } else {
            Self {
                chat_area: top_area,
                status_bar_area,
                sidebar_area: None,
                input_area,
            }
        }
    }
}
