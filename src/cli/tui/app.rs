use std::io;
use std::io::stdout;
use std::process::Command;
use std::rc::Rc;

use ratatui::{
    backend::Backend,
    buffer::Buffer,
    crossterm::{
        event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
        terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
        ExecutableCommand,
    },
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Style, Stylize},
    symbols,
    text::{Line, Span, Text},
    widgets::{
        Block, Borders, HighlightSpacing, List, ListItem, ListState, Paragraph, StatefulWidget,
        Widget, Wrap,
    },
    Frame, Terminal,
};

use super::input::{Input, InputFor};
use super::popup::{Popup, PopupFor};
use todoozy::todo::filter;
use todoozy::todo::sort;
use todoozy::Todo;

const SELECTED_STYLE: Style = Style::new().fg(Color::Black).bg(Color::Green);
const TEXT_FG_COLOR: Color = Color::White;

#[derive(Debug, Default)]
struct TodoList {
    items: Vec<TodoItem>,
    state: ListState,
}

impl TodoList {
    fn new(
        todo_view: Vec<Rc<Todo>>,
        filter: &Box<dyn filter::Filter>,
        sorter: &Box<dyn sort::Sorter>,
    ) -> Self {
        let mut state = ListState::default();

        let mut items: Vec<TodoItem> = todo_view
            .into_iter()
            .filter(|t| filter.filter(t))
            .map(|todo| TodoItem::new(Status::Todo, todo))
            .collect();

        items.sort_unstable_by(|a, b| sorter.compare(&a.todo, &b.todo));

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
struct TodoItem {
    todo: Rc<Todo>,
    status: Status,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Status {
    Todo,
    Completed,
}

impl TodoItem {
    fn new(status: Status, todo: Rc<Todo>) -> Self {
        Self { status, todo }
    }
}

/// This struct holds the current state of the app. In particular, it has the `todo_list` field
/// which is a wrapper around `ListState`. Keeping track of the state lets us render the
/// associated widget with its state and have access to features such as natural scrolling.
///
/// Check the event handling at the bottom to see how to change the state on incoming events. Check
/// the drawing logic for items on how to specify the highlighting style for selected items.
pub struct App {
    should_exit: bool,

    /// A list of files to exclude from the search.
    exclude: Vec<String>,

    /// The complete list of todos that this app manages.
    todo_view: Vec<Rc<Todo>>,
    unowned_todos: Vec<Rc<Todo>>,

    /// The list of todos that are currently displayed after filters / sorts have been applied.
    todo_list: TodoList,
    selected: Option<usize>,

    filter: Box<dyn filter::Filter>,

    sorter: Box<dyn sort::Sorter>,

    input: Option<Input>,
    input_for: Option<InputFor>,

    popup: Option<Popup>,
    popup_for: Option<PopupFor>,
}

use crate::cli::config::Config;
impl App {
    pub fn new(mut config: Config) -> Result<Self, Box<dyn std::error::Error>> {
        // Start up admin
        let todos = todoozy::get_todos(&config.exclude).unwrap();
        let max_id = std::cmp::max(todos.get_max_id(), config.num_todos);
        if max_id > config.num_todos {
            config.num_todos = max_id;
            config.save()?;
        }

        // Assign IDs to todos that don't have them.
        let mut unowned_todos: Vec<Rc<Todo>> = Vec::new();
        let todo_view: Vec<Rc<Todo>> = todos
            .into_iter()
            .map(|mut t| {
                let mut updated = false;
                if t.id.is_none() {
                    config.num_todos += 1;
                    t.id = Some(config.num_todos);
                    updated = true;
                }
                let rc = Rc::new(t);
                if updated {
                    unowned_todos.push(rc.clone());
                }
                rc
            })
            .collect();

        let (popup, popup_for) = if unowned_todos.is_empty() {
            (None, None)
        } else {
            (Some(Popup::new(60, 20)), Some(PopupFor::UnownedTodos))
        };

        let mut app = Self {
            should_exit: false,
            exclude: config.exclude,
            todo_view,
            unowned_todos,
            todo_list: TodoList::default(),
            selected: None,
            filter: config.filter.unwrap_or(Box::new(filter::All::default())),
            sorter: config
                .sorter
                .unwrap_or(Box::new(sort::SortPipeline::app_default())),
            input: None,
            input_for: None,
            popup,
            popup_for,
        };

        app.todo_list = TodoList::new(app.todo_view.clone(), &app.filter, &app.sorter);

        Ok(app)
    }

    pub fn run(&mut self, mut terminal: Terminal<impl Backend>) -> io::Result<()> {
        while !self.should_exit {
            terminal.draw(|f| {
                f.render_widget(&mut *self, f.area());
                self.set_cursor_position(f);

                if let Some(ref popup) = &self.popup {
                    match &self.popup_for {
                        Some(PopupFor::UnownedTodos) => {
                            let content = Popup::unowned_todos(&self.todo_view);
                            popup.render(f, content);
                        }
                        None => {}
                    }
                }
            })?;
            if let Event::Key(key) = event::read()? {
                self.handle_key(key, &mut terminal);
            };
        }
        Ok(())
    }

    fn set_cursor_position(&mut self, frame: &mut Frame) {
        if let Some(ref input) = self.input {
            input.set_cursor_position(frame);
        }
    }

    fn handle_key(&mut self, key: KeyEvent, terminal: &mut Terminal<impl Backend>) {
        if key.kind != KeyEventKind::Press {
            return;
        }

        // TODO (Z) 2024-08-23 A way to scroll the list view to the right
        //
        // So we can see all the info of long todos who's information can't fit on the current
        // terminal width.
        //
        // Putting this in the backlog for now because file formatting will mean that there
        // shouldn't be any massive long todos. If there are then you can always press enter to
        // view the whole todo expanded.
        match &self.popup {
            Some(_) => match key.code {
                KeyCode::Char('y') => match &self.popup_for {
                    Some(PopupFor::UnownedTodos) => {
                        self.import_todos();
                        self.popup = None;
                        self.popup_for = None;
                    }
                    None => unreachable!(),
                },
                KeyCode::Esc | KeyCode::Char('n') => self.popup = None,
                _ => {}
            },
            None => match self.input {
                None => match key.code {
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
                    KeyCode::Char('f') => self.get_input(InputFor::Filter),
                    KeyCode::Char('F') => self.reset_filter(),
                    KeyCode::Char('g') | KeyCode::Home => self.select_first(),
                    KeyCode::Char('G') | KeyCode::End => self.select_last(),
                    KeyCode::Enter => {
                        self.selected = self.todo_list.state.selected();
                    }
                    KeyCode::Char('l') | KeyCode::Right => self.toggle_status(),
                    KeyCode::Char('R') => self.refresh_todos(),
                    KeyCode::Char('s') => self.get_input(InputFor::Sort),
                    KeyCode::Char('S') => self.reset_sort(),
                    _ => {}
                },
                Some(ref mut input) => match key.code {
                    KeyCode::Enter => {
                        let output = input.submit();
                        self.handle_input(output);
                        self.input = None;
                    }
                    KeyCode::Char(to_insert) => input.enter_char(to_insert),
                    KeyCode::Backspace => input.delete_char(),
                    KeyCode::Left => input.move_cursor_left(),
                    KeyCode::Right => input.move_cursor_right(),
                    KeyCode::Esc => self.input = None,
                    _ => {}
                },
            },
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

    fn get_input(&mut self, input_for: InputFor) {
        self.input = match input_for {
            InputFor::Filter => Some(Input::new("filter:".to_string())),
            InputFor::Sort => Some(Input::new("sort:".to_string())),
        };
        self.input_for = Some(input_for);
    }

    fn handle_input(&mut self, input: String) {
        match self.input_for {
            Some(InputFor::Filter) => self.set_filter(input),
            Some(InputFor::Sort) => self.set_sort(input),
            None => {}
        };
    }

    fn set_filter(&mut self, filter: String) {
        match filter::parse_str(filter) {
            Ok(f) => {
                self.filter = f;
                self.todo_list = TodoList::new(self.todo_view.clone(), &self.filter, &self.sorter);
            }
            Err(_) => {}
        };
    }

    fn reset_filter(&mut self) {
        self.filter = Box::new(filter::All::default());
        self.todo_list = TodoList::new(self.todo_view.clone(), &self.filter, &self.sorter);
    }

    fn set_sort(&mut self, sort: String) {
        match sort::parse_str(sort) {
            Ok(s) => {
                self.sorter = s;
                self.todo_list = TodoList::new(self.todo_view.clone(), &self.filter, &self.sorter);
            }
            Err(_) => {}
        };
    }

    fn reset_sort(&mut self) {
        self.sorter = Box::new(sort::SortPipeline::app_default());
        self.todo_list = TodoList::new(self.todo_view.clone(), &self.filter, &self.sorter);
    }

    fn refresh_todos(&mut self) {
        let todo_data = todoozy::get_todos(&self.exclude).unwrap();
        self.todo_view = todo_data.into_iter().map(|t| Rc::new(t)).collect();
        self.todo_list = TodoList::new(self.todo_view.clone(), &self.filter, &self.sorter);
    }

    fn import_todos(&mut self) {
        for todo in &self.unowned_todos {
            match todo.write_id() {
                Ok(_) => {}
                Err(e) => eprintln!("Error writing todo: {}", e),
            }
        }
        //match todoozy::import_todos(&self.unowned_todos) {
        //    Ok(_) => {
        //        self.unowned_todos.clear();
        //        self.popup = None;
        //        self.popup_for = None;
        //    }
        //    Err(e) => eprintln!("Error importing todos: {}", e),
        //}
    }
}

impl Widget for &mut App {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let [main_area, footer_area, input_area] = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(1),
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

        self.render_footer(footer_area, buf);

        if let Some(ref mut input) = self.input {
            input.render(input_area, buf);
        }
    }
}

fn num_digits(n: u32) -> u32 {
    if n == 0 {
        return 1;
    }
    ((n as f64).log10() + 1.0).floor() as u32
}

#[test]
fn test_num_digits() {
    assert_eq!(num_digits(0), 1);
    assert_eq!(num_digits(1), 1);
    assert_eq!(num_digits(9), 1);
    assert_eq!(num_digits(10), 2);
    assert_eq!(num_digits(99), 2);
    assert_eq!(num_digits(100), 3);
}

impl App {
    fn render_footer(&mut self, area: Rect, buf: &mut Buffer) {
        let [left, right] =
            Layout::horizontal([Constraint::Fill(1), Constraint::Fill(1)]).areas(area);

        Paragraph::new(format!("{}", self.filter))
            .bg(Color::Magenta)
            .fg(Color::Black)
            .render(left, buf);
        Paragraph::new(format!("{}", self.sorter))
            .bg(Color::Magenta)
            .fg(Color::Black)
            .alignment(Alignment::Right)
            .render(right, buf);
    }

    fn render_list(&mut self, area: Rect, buf: &mut Buffer) {
        let short_paths: Vec<String> = self
            .todo_list
            .items
            .iter()
            .map(|t| crate::cli::display::truncate_path(&t.todo.location_start()))
            .collect();
        let max_path_width = short_paths.iter().map(|s| s.len()).max().unwrap_or(0);
        let max_id = self
            .todo_list
            .items
            .iter()
            .map(|t| t.todo.id.unwrap_or(0))
            .max()
            .unwrap_or(0);
        let max_id_digits = num_digits(max_id);

        let items: Vec<ListItem> = self
            .todo_list
            .items
            .iter()
            .map(|todo_item| App::make_listitem(todo_item, max_id_digits, max_path_width))
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
        let todo = &self.todo_list.selected().unwrap().todo;

        let block = Block::new()
            .title(Line::raw(format!("[info]")).left_aligned())
            .borders(Borders::TOP)
            .border_set(symbols::border::EMPTY)
            .border_style(Style::new().fg(Color::Black).bg(Color::Blue));

        let mut text = Text::default();

        let mut spans: Vec<Span> = Vec::new();
        if let Some(id) = &todo.id {
            spans.push(Span::styled(
                format!("#{} ", id),
                Style::new().fg(Color::Red),
            ));
        }
        spans.push(Span::styled(
            todo.title.clone(),
            Style::new().fg(Color::Green),
        ));
        text.push_line(Line::default().spans(spans));

        text.push_line("\n");

        // TODO (E) 2024-08-15 Make it so the status reflects todos done since opening the tui
        // +feature
        //
        // While you're editing todos in the app it keeps track of which ones you've deleted since
        // you started your session. This requires all todos to have an ID so that we can actually
        // keep track of which todo is which.
        //
        // This feature needs some work before implementing. Philosophically, the old todos are
        // being managed in the version control system anyway. Having a way of viewing old todos
        // (i.e. any todos that appear in the commit history) is a must then. If a user adds a todo
        // while coding and then deletes it before they come to commit, then I kinda think it
        // wasn't important enough to capture ¯\_(ツ)_/¯
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

        if let Some(creation_date) = todo.creation_date {
            text.push_line(Line::styled(
                format!("creation_date: {}", creation_date),
                Style::new().fg(Color::Red),
            ));
        }

        text.push_line("\n");

        let mut has_metadata = false;
        for (key, value) in todo.metadata.iter() {
            has_metadata = true;
            text.push_line(Line::styled(
                format!("{}: {}", key, value),
                Style::new().fg(Color::Cyan),
            ));
        }

        if has_metadata {
            text.push_line("\n");
        }

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

    fn make_listitem<'a>(
        todo_item: &TodoItem,
        max_id_digits: u32,
        max_path_width: usize,
    ) -> ListItem<'a> {
        let mut location =
            crate::cli::display::truncate_path(todo_item.todo.location_start().as_str());
        if location.len() < max_path_width {
            location.push_str(&" ".repeat(max_path_width - location.len()));
        }

        let projects: Vec<Span> = todo_item
            .todo
            .projects
            .iter()
            .map(|p| Span::styled(format!(" +{}", p), Style::new().fg(Color::Magenta)))
            .collect();

        let contexts: Vec<Span> = todo_item
            .todo
            .contexts
            .iter()
            .map(|p| Span::styled(format!(" @{}", p), Style::new().fg(Color::Cyan)))
            .collect();

        let line = Line::from(
            vec![
                Span::styled(
                    format!(
                        "#{:<width$} ",
                        todo_item.todo.id.unwrap_or(0),
                        width = max_id_digits as usize
                    ),
                    Style::new().fg(Color::Green),
                ),
                // TODO #2 (E) 2024-09-05 What are we going to do with the [ ] checkbox in the UI?
                //
                // Not sure how useful this is as toggling the status of todos from the UI is still
                // not well defined. It would be nice to see in progress etc. especially if someone
                // was working on one on another branch but it wasn't finished yet and they're
                // partially committed their work.
                //
                // Currently, we just have a list, and when you complete stuff it disappears, which
                // feels like it might not be the most satisfying experience (although it has been
                // motivating me for a few weeks now on this project).
                Span::styled("[ ] ", Style::new().fg(Color::Red)),
                Span::styled(format!("{} ", location), Style::new().fg(Color::Blue)),
                Span::styled(
                    format!("({}) ", todo_item.todo.priority.unwrap_or('Z')),
                    Style::new().fg(Color::Yellow),
                ),
                Span::styled(todo_item.todo.title.clone(), Style::new().fg(Color::White)),
            ]
            .into_iter()
            .chain(projects.into_iter())
            .chain(contexts.into_iter())
            .collect::<Vec<Span>>(),
        );

        ListItem::new(line)
    }
}
