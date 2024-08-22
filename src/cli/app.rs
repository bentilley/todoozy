use std::io;
use std::io::stdout;
use std::process::Command;
// use std::{error::Error, io};

use ratatui::{
    backend::Backend,
    buffer::Buffer,
    crossterm::{
        event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
        terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
        ExecutableCommand,
    },
    layout::{Constraint, Layout, Rect},
    style::{Color, Style, Stylize},
    symbols,
    text::{Line, Span, Text},
    widgets::{
        Block, Borders, HighlightSpacing, List, ListItem, ListState, Paragraph, StatefulWidget,
        Widget, Wrap,
    },
    Terminal,
};

const SELECTED_STYLE: Style = Style::new().fg(Color::Black).bg(Color::Green);
const TEXT_FG_COLOR: Color = Color::White;

/// This struct holds the current state of the app. In particular, it has the `todo_list` field
/// which is a wrapper around `ListState`. Keeping track of the state lets us render the
/// associated widget with its state and have access to features such as natural scrolling.
///
/// Check the event handling at the bottom to see how to change the state on incoming events. Check
/// the drawing logic for items on how to specify the highlighting style for selected items.
pub struct App<'a> {
    should_exit: bool,
    /// The complete list of todos that this app manages.
    todo_data: &'a [todoozy::Todo],

    /// The list of todos that are currently displayed after filters / sorts have been applied.
    todo_list: TodoList<'a>,
    selected: Option<usize>,

    filter: Box<dyn todoozy::filter::Filter>,
    sorter: Box<dyn todoozy::sort::Sorter>,
}

pub struct AppConfig {
    pub filter: Box<dyn todoozy::filter::Filter>,
    pub sorter: Box<dyn todoozy::sort::Sorter>,
}

struct TodoList<'a> {
    items: Vec<TodoItem<'a>>,
    state: ListState,
}

impl<'a> TodoList<'a> {
    fn new(items: Vec<TodoItem<'a>>) -> Self {
        let mut state = ListState::default();
        if !items.is_empty() {
            state.select(Some(0));
        }
        Self { items, state }
    }

    fn selected(&self) -> Option<&TodoItem> {
        match self.state.selected() {
            Some(i) => Some(&self.items[i]),
            None => None,
        }
    }
}

#[derive(Debug, Clone)]
struct TodoItem<'a> {
    todo: &'a todoozy::Todo,
    status: Status,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Status {
    Todo,
    Completed,
}

impl<'a> TodoItem<'a> {
    fn new(status: Status, todo: &'a todoozy::Todo) -> Self {
        Self { status, todo }
    }
}

impl<'a> App<'a> {
    pub fn new(config: AppConfig, todo_data: &'a [todoozy::Todo]) -> Self {
        let mut app = Self {
            should_exit: false,
            todo_data,
            todo_list: TodoList::new(Vec::new()),
            selected: None,
            filter: config.filter,
            sorter: config.sorter,
        };
        app.filter_todo_list();
        app.sort_todo_list();
        app
    }

    fn filter_todo_list(&mut self) {
        let items: Vec<TodoItem> = self
            .todo_data
            .iter()
            .filter(|t| self.filter.filter(t))
            .map(|todo| TodoItem::new(Status::Todo, todo))
            .collect();
        self.todo_list = TodoList::new(items);
    }

    fn sort_todo_list(&mut self) {
        self.todo_list
            .items
            .sort_unstable_by(|a, b| self.sorter.compare(a.todo, b.todo));
    }

    pub fn run(&mut self, mut terminal: Terminal<impl Backend>) -> io::Result<()> {
        while !self.should_exit {
            terminal.draw(|f| f.render_widget(&mut *self, f.area()))?;
            if let Event::Key(key) = event::read()? {
                self.handle_key(key, &mut terminal);
            };
        }
        Ok(())
    }

    fn handle_key(&mut self, key: KeyEvent, terminal: &mut Terminal<impl Backend>) {
        if key.kind != KeyEventKind::Press {
            return;
        }

        // TODO (B) 2024-08-22 Add a way to refresh the current todo list. ODOT
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                if self.selected.is_some() {
                    self.selected = None;
                } else {
                    self.should_exit = true
                }
            }
            KeyCode::Char('e') => self.edit_selected(terminal).unwrap(),
            KeyCode::Char('h') | KeyCode::Left => self.select_none(),
            KeyCode::Char('j') | KeyCode::Down => self.select_next(),
            KeyCode::Char('k') | KeyCode::Up => self.select_previous(),
            KeyCode::Char('g') | KeyCode::Home => self.select_first(),
            KeyCode::Char('G') | KeyCode::End => self.select_last(),
            KeyCode::Enter => {
                self.selected = self.todo_list.state.selected();
            }
            KeyCode::Char('l') | KeyCode::Right => self.toggle_status(),
            _ => {}
        }
    }

    fn select_none(&mut self) {
        self.todo_list.state.select(None);
    }

    fn select_next(&mut self) {
        self.todo_list.state.select_next();
    }
    fn select_previous(&mut self) {
        self.todo_list.state.select_previous();
    }

    fn select_first(&mut self) {
        self.todo_list.state.select_first();
    }

    fn select_last(&mut self) {
        self.todo_list.state.select_last();
    }

    fn edit_selected(&mut self, terminal: &mut Terminal<impl Backend>) -> io::Result<()> {
        let editor = std::env::var("EDITOR").unwrap_or("vi".to_string());

        if let Some(item) = self.todo_list.selected() {
            if let Some(file) = &item.todo.file {
                stdout().execute(LeaveAlternateScreen)?;
                disable_raw_mode()?;
                let _ = Command::new(editor)
                    .arg(file)
                    .arg(format!(
                        "+{}",
                        item.todo.line_number.unwrap_or(1).to_string()
                    ))
                    .status();
                stdout().execute(EnterAlternateScreen)?;
                enable_raw_mode()?;
                terminal.clear()?;
            }
        }
        Ok(())
    }

    /// Changes the status of the selected list item
    fn toggle_status(&mut self) {
        if let Some(i) = self.todo_list.state.selected() {
            self.todo_list.items[i].status = match self.todo_list.items[i].status {
                Status::Completed => Status::Todo,
                Status::Todo => Status::Completed,
            }
        }
    }
}

impl Widget for &mut App<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let [main_area, footer_area] = Layout::vertical([
            // Constraint::Length(2),
            Constraint::Fill(1),
            Constraint::Length(1),
        ])
        .areas(area);

        match self.selected {
            Some(_) => {
                let [list_area, item_area] =
                    Layout::vertical([Constraint::Fill(1), Constraint::Fill(1)]).areas(main_area);

                self.render_list(list_area, buf);
                self.render_selected_item(item_area, buf);
            }
            None => {
                self.render_list(main_area, buf);
            }
        }

        App::render_footer(footer_area, buf);
    }
}

impl App<'_> {
    // TODO (B) 2024-08-22 Fix the instructions in the footer ODOT
    fn render_footer(area: Rect, buf: &mut Buffer) {
        Paragraph::new("Use ↓↑ to move, ← to unselect, → to change status, g/G to go top/bottom.")
            .bg(Color::Magenta)
            .fg(Color::Black)
            .centered()
            .render(area, buf);
    }

    fn render_list(&mut self, area: Rect, buf: &mut Buffer) {
        let items: Vec<ListItem> = self
            .todo_list
            .items
            .iter()
            .enumerate()
            .map(|(_i, todo_item)| ListItem::from(todo_item))
            .collect();

        let list = List::new(items)
            .highlight_style(SELECTED_STYLE)
            .highlight_symbol(">")
            .highlight_spacing(HighlightSpacing::Always);

        // We need to disambiguate this trait method as both `Widget` and `StatefulWidget` share the
        // same method name `render`.
        StatefulWidget::render(list, area, buf, &mut self.todo_list.state);
    }

    fn render_selected_item(&self, area: Rect, buf: &mut Buffer) {
        let todo = self.todo_list.selected().unwrap().todo;

        let block = Block::new()
            .title(Line::raw(format!("[info]")).left_aligned())
            .borders(Borders::TOP)
            .border_set(symbols::border::EMPTY)
            .border_style(Style::new().fg(Color::Black).bg(Color::Blue));

        let mut text = Text::default();

        text.push_line(Line::styled(
            todo.title.clone(),
            Style::new().fg(Color::Green),
        ));

        // TODO (E) 2024-08-15 Make it so the status is actually toggle-able +features
        //
        // This needs some work on the parser - I guess we keep the status in the todo, before the
        // priority, as something like [x] or [ ] or [@], etc. Then the big win would be allowing
        // that to be toggled via the TUI... :)
        //
        // Or, maybe while you're editing todos in the app it keeps track of which ones you've
        // deleted since you started your session...?
        //
        // This feature needs some work before implementing. Philosophically, the old todos are
        // being managed in the version control system anyway. Having a way of viewing old todos
        // (i.e. any todos that appear in the commit history) is a must then. If a user adds a todo
        // while coding and then deletes it before they come to commit, then I kinda think it
        // wasn't important enough to capture ¯\_(ツ)_/¯
        // ODOT
        text.push_line(Line::styled(
            "status: in progress",
            Style::new().fg(Color::Magenta),
        ));

        if let Some(ref file) = todo.file {
            let mut t = "location: ".to_string();
            t.push_str(file);
            if let Some(line_number) = todo.line_number {
                t.push_str(&format!(":{}", line_number));
            }
            text.push_line(Line::styled(t, Style::new().fg(Color::Blue)));
        }

        if let Some(priority) = todo.priority {
            text.push_line(Line::styled(
                format!("priority: ({}) ", priority),
                Style::new().fg(Color::Yellow),
            ));
        }

        text.push_line("\n");

        if let Some(ref description) = todo.description {
            for line in Text::from(description.clone()).iter() {
                text.push_line(line.clone());
            }
        }

        Paragraph::new(text)
            .block(block)
            .fg(TEXT_FG_COLOR)
            .wrap(Wrap { trim: false })
            .render(area, buf);
    }
}

impl From<&TodoItem<'_>> for ListItem<'_> {
    fn from(value: &TodoItem) -> Self {
        let mut location = match value.todo.file {
            Some(ref file) => match value.todo.line_number {
                Some(line_number) => format!("{}:{}", file, line_number),
                None => file.to_string(),
            },
            None => "".to_string(),
        };
        // TODO (A) 2024-08-22 Fix the cropping of the location. +bug
        //
        // This naive cropping is going to run out of space in no time (it already is).
        // ODOT
        if location.len() < 20 {
            location.push_str(&" ".repeat(20 - location.len()));
        }
        location.truncate(20);

        // TODO (A) 2024-08-22 Render the projects (and maybe context...) in the item list
        // + feature
        // ODOT

        let line = Line::from(vec![
            Span::styled("[ ] ", Style::new().fg(Color::Magenta)),
            Span::styled(format!("{} ", location), Style::new().fg(Color::Blue)),
            Span::styled(
                format!("({}) ", value.todo.priority.unwrap_or('Z')),
                Style::new().fg(Color::Yellow),
            ),
            Span::styled(
                format!("{}", value.todo.title),
                Style::new().fg(Color::White),
            ),
        ]);

        // let line = match value.status {
        //     Status::Todo => Line::styled(format!(" ☐ {}", value.todo.title), TEXT_FG_COLOR),
        //     Status::Completed => {
        //         Line::styled(format!(" ✓ {}", value.todo.title), COMPLETED_TEXT_FG_COLOR)
        //     }
        // };

        ListItem::new(line)
    }
}

// mod tui {
//     use std::{io, io::stdout};

//     use color_eyre::config::HookBuilder;
//     use ratatui::{
//         backend::{Backend, CrosstermBackend},
//         crossterm::{
//             terminal::{
//                 disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
//             },
//             ExecutableCommand,
//         },
//         Terminal,
//     };

//     pub fn init_error_hooks() -> color_eyre::Result<()> {
//         let (panic, error) = HookBuilder::default().into_hooks();
//         let panic = panic.into_panic_hook();
//         let error = error.into_eyre_hook();
//         color_eyre::eyre::set_hook(Box::new(move |e| {
//             let _ = restore_terminal();
//             error(e)
//         }))?;
//         std::panic::set_hook(Box::new(move |info| {
//             let _ = restore_terminal();
//             panic(info);
//         }));
//         Ok(())
//     }

//     pub fn init_terminal() -> io::Result<Terminal<impl Backend>> {
//         stdout().execute(EnterAlternateScreen)?;
//         enable_raw_mode()?;
//         Terminal::new(CrosstermBackend::new(stdout()))
//     }

//     pub fn restore_terminal() -> io::Result<()> {
//         stdout().execute(LeaveAlternateScreen)?;
//         disable_raw_mode()
//     }
// }
