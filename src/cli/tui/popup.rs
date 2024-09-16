use ratatui::{
    layout::{Alignment, Constraint, Flex, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph},
    Frame,
};
use std::rc::Rc;
use todoozy::Todo;

pub enum PopupFor {
    UnownedTodos,
}

/// helper function to create a centered rect using up certain percentage of the available rect `r`
fn popup_area(area: Rect, percent_x: u16, percent_y: u16) -> Rect {
    let vertical = Layout::vertical([Constraint::Percentage(percent_y)]).flex(Flex::Center);
    let horizontal = Layout::horizontal([Constraint::Percentage(percent_x)]).flex(Flex::Center);
    let [area] = vertical.areas(area);
    let [area] = horizontal.areas(area);
    area
}

pub enum Content<'a> {
    UnownedTodos(u16, Paragraph<'a>),
}

/// Popup is a UI component for displaying popups.
pub struct Popup {
    percent_x: u16,
    percent_y: u16,
}

impl Popup {
    pub fn new(percent_x: u16, percent_y: u16) -> Self {
        Self {
            percent_x,
            percent_y,
        }
    }

    pub fn render(&self, frame: &mut Frame, content: Content) {
        let area = popup_area(frame.area(), self.percent_x, self.percent_y);
        let height = area.height;
        let content_height = match &content {
            Content::UnownedTodos(height, _) => *height,
        };

        let vertical_padding = ((height - 2) / 2) - (content_height / 2);

        let block = Block::default()
            .title("Import TODOs")
            .style(Style::default().bg(Color::Black).fg(Color::Blue))
            .padding(Padding::vertical(vertical_padding))
            .border_type(BorderType::Rounded)
            .borders(Borders::ALL);
        let content_area = block.inner(area);

        frame.render_widget(Clear, area); //this clears out the background
        frame.render_widget(block, area);

        match &content {
            Content::UnownedTodos(_, content) => {
                frame.render_widget(content, content_area);
            }
        }
    }

    pub fn unowned_todos<'b>(unowned_todos: &[Rc<Todo>]) -> Content<'b> {
        let num_todos = unowned_todos.len();
        Content::UnownedTodos(
            3,
            Paragraph::new(vec![
                Line::from(format!("{} TODOs need importing.", num_todos)),
                Line::default(),
                Line::from(vec![
                    Span::styled("y", Style::default().fg(Color::Green)),
                    Span::from(": confirm / "),
                    Span::styled("n", Style::default().fg(Color::Red)),
                    Span::from(": cancel"),
                ]),
            ])
            .style(Style::default().bg(Color::Black).fg(Color::White))
            .alignment(Alignment::Center),
        )
    }
}
