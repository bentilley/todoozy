use ratatui::{
    buffer::Buffer,
    layout::{Position, Rect},
    style::{Color, Style},
    widgets::{Paragraph, Widget},
    Frame,
};

// pub enum InputMode {
//     Normal,
//     Editing,
// }

/// Input is a UI component for capturing user input.
pub struct Input {
    /// The character to use as the input prompt
    prompt: String,
    prompt_length: u16,
    /// Current value of the input box
    input: String,
    /// Position of cursor in the editor area.
    character_index: usize,
    cursor_position: Position,
    // /// Current input mode
    // input_mode: InputMode,
    // /// History of recorded messages
    // messages: Vec<String>,
}

impl Input {
    pub fn new(prompt: String) -> Self {
        Self {
            prompt_length: prompt.len() as u16,
            prompt,
            input: String::new(),
            // input_mode: InputMode::Normal,
            // messages: Vec::new(),
            character_index: 0,
            cursor_position: Position::default(),
        }
    }

    pub fn move_cursor_left(&mut self) {
        let cursor_moved_left = self.character_index.saturating_sub(1);
        self.character_index = self.clamp_cursor(cursor_moved_left);
    }

    pub fn move_cursor_right(&mut self) {
        let cursor_moved_right = self.character_index.saturating_add(1);
        self.character_index = self.clamp_cursor(cursor_moved_right);
    }

    pub fn enter_char(&mut self, new_char: char) {
        let index = self.byte_index();
        self.input.insert(index, new_char);
        self.move_cursor_right();
    }

    /// Returns the byte index based on the character position.
    ///
    /// Since each character in a string can be contain multiple bytes, it's necessary to calculate
    /// the byte index based on the index of the character.
    fn byte_index(&self) -> usize {
        self.input
            .char_indices()
            .map(|(i, _)| i)
            .nth(self.character_index)
            .unwrap_or(self.input.len())
    }

    pub fn delete_char(&mut self) {
        let is_not_cursor_leftmost = self.character_index != 0;
        if is_not_cursor_leftmost {
            // Method "remove" is not used on the saved text for deleting the selected char.
            // Reason: Using remove on String works on bytes instead of the chars.
            // Using remove would require special care because of char boundaries.

            let current_index = self.character_index;
            let from_left_to_current_index = current_index - 1;

            // Getting all characters before the selected character.
            let before_char_to_delete = self.input.chars().take(from_left_to_current_index);
            // Getting all characters after selected character.
            let after_char_to_delete = self.input.chars().skip(current_index);

            // Put all characters together except the selected one.
            // By leaving the selected one out, it is forgotten and therefore deleted.
            self.input = before_char_to_delete.chain(after_char_to_delete).collect();
            self.move_cursor_left();
        }
    }

    fn clamp_cursor(&self, new_cursor_pos: usize) -> usize {
        new_cursor_pos.clamp(0, self.input.chars().count())
    }

    fn reset_cursor(&mut self) {
        self.character_index = 0;
    }

    pub fn submit(&mut self) -> String {
        let input = self.input.clone();
        self.input.clear();
        self.reset_cursor();
        input
    }

    // fn run(mut self, mut terminal: DefaultTerminal) -> Result<()> {
    //     loop {
    //         terminal.draw(|frame| self.draw(frame))?;

    //         if let Event::Key(key) = event::read()? {
    //             match self.input_mode {
    //                 InputMode::Normal => match key.code {
    //                     KeyCode::Char('e') => {
    //                         self.input_mode = InputMode::Editing;
    //                     }
    //                     KeyCode::Char('q') => {
    //                         return Ok(());
    //                     }
    //                     _ => {}
    //                 },
    //                 InputMode::Editing if key.kind == KeyEventKind::Press => match key.code {
    //                     KeyCode::Enter => self.submit_message(),
    //                     KeyCode::Char(to_insert) => self.enter_char(to_insert),
    //                     KeyCode::Backspace => self.delete_char(),
    //                     KeyCode::Left => self.move_cursor_left(),
    //                     KeyCode::Right => self.move_cursor_right(),
    //                     KeyCode::Esc => self.input_mode = InputMode::Normal,
    //                     _ => {}
    //                 },
    //                 InputMode::Editing => {}
    //             }
    //         }
    //     }
    // }

    pub fn set_cursor_position(&self, frame: &mut Frame) {
        frame.set_cursor_position(self.cursor_position);
    }
}

impl Widget for &mut Input {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mut text = self.prompt.to_string();
        text.push(' ');
        text.push_str(&self.input);
        Paragraph::new(text)
            .style(Style::default().bg(Color::Magenta).fg(Color::Black))
            // .style(match self.input_mode {
            //     InputMode::Normal => Style::default(),
            //     InputMode::Editing => Style::default().fg(Color::Yellow),
            // })
            // .block(Block::bordered().title("Input"))
            .render(area, buf);

        self.cursor_position = Position::new(
            area.x + self.character_index as u16 + self.prompt_length + 1,
            area.y,
            // Move one line down, from the border to the input line
            // area.y + 1,
        )
    }
}
