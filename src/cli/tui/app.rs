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
pub struct App {
    should_exit: bool,

    /// A list of files to exclude from the search.
    exclude: Vec<String>,

    /// The complete list of todos that this app manages.
    todo_view: Vec<Rc<todoozy::Todo>>,

    /// The list of todos that are currently displayed after filters / sorts have been applied.
    todo_list: TodoList,
    selected: Option<usize>,

    filter: Box<dyn todoozy::filter::Filter>,
    sorter: Box<dyn todoozy::sort::Sorter>,
}

pub struct AppConfig {
    pub exclude: Vec<String>,
    pub filter: Box<dyn todoozy::filter::Filter>,
    pub sorter: Box<dyn todoozy::sort::Sorter>,
}

#[derive(Debug, Default)]
struct TodoList {
    items: Vec<TodoItem>,
    state: ListState,
}

impl TodoList {
    fn new(
        todo_view: Vec<Rc<todoozy::Todo>>,
        filter: &Box<dyn todoozy::filter::Filter>,
        sorter: &Box<dyn todoozy::sort::Sorter>,
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

    // fn sort(&mut self, sorter: &Box<dyn todoozy::sort::Sorter>) {
    //     self.items
    //         .sort_unstable_by(|a, b| sorter.compare(a.todo, b.todo));
    // }
}

#[derive(Debug, Clone)]
struct TodoItem {
    todo: Rc<todoozy::Todo>,
    status: Status,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Status {
    Todo,
    Completed,
}

impl TodoItem {
    fn new(status: Status, todo: Rc<todoozy::Todo>) -> Self {
        Self { status, todo }
    }
}

impl App {
    pub fn new(config: AppConfig, todo_data: Vec<todoozy::Todo>) -> Self {
        let mut app = Self {
            should_exit: false,
            exclude: config.exclude,
            todo_view: todo_data.into_iter().map(|t| Rc::new(t)).collect(),
            todo_list: TodoList::default(),
            selected: None,
            filter: config.filter,
            sorter: config.sorter,
        };

        app.todo_list = TodoList::new(app.todo_view.clone(), &app.filter, &app.sorter);

        app
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

        // TODO (C) 2024-08-23 A way to scroll the list view to the right
        //
        // So we can see all the info of long todos who's information can't fit on the current
        // terminal width.
        // ODOT
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
            KeyCode::Char('R') => self.refresh_todos(),
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

    fn refresh_todos(&mut self) {
        let todo_data = todoozy::get_todos(&self.exclude).unwrap();
        self.todo_view = todo_data.into_iter().map(|t| Rc::new(t)).collect();
        self.todo_list = TodoList::new(self.todo_view.clone(), &self.filter, &self.sorter);
    }
}

impl Widget for &mut App {
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

impl App {
    // TODO (B) 2024-08-22 Fix the instructions in the footer ODOT
    fn render_footer(area: Rect, buf: &mut Buffer) {
        Paragraph::new("Use ↓↑ to move, ← to unselect, → to change status, g/G to go top/bottom.")
            .bg(Color::Magenta)
            .fg(Color::Black)
            .centered()
            .render(area, buf);
    }

    fn render_list(&mut self, area: Rect, buf: &mut Buffer) {
        let short_paths: Vec<String> = self
            .todo_list
            .items
            .iter()
            .map(|t| crate::cli::display::truncate_path(&t.todo.location_start()))
            .collect();
        let max_path_width = short_paths.iter().map(|s| s.len()).max().unwrap_or(0);

        let items: Vec<ListItem> = self
            .todo_list
            .items
            .iter()
            .map(|todo_item| make_listitem(todo_item, max_path_width))
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

fn make_listitem<'a>(todo_item: &TodoItem, max_path_width: usize) -> ListItem<'a> {
    let mut location = crate::cli::display::truncate_path(todo_item.todo.location_start().as_str());
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
