//! Ratatui-based TUI. Entry point is [`Tui::start`].
use std::{
    collections::{HashMap, HashSet},
    fmt::Display,
    fs::File,
    io::Write,
    str::FromStr,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use anyhow::anyhow;
use chrono::Utc;
use crossbeam_channel::Receiver;
use crossterm::{
    ExecutableCommand,
    event::{self, KeyCode, KeyModifiers},
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Position},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, List, ListItem, Paragraph},
};

use crate::{
    event::Event,
    matcher::{FuzzyEngine, FuzzyIndex, Match},
};

struct EventReader {
    // TODO this is a bit ugly can we refactor this?
    // maybe Cow is enough here
    buffer: Arc<Mutex<Vec<Arc<Event>>>>,
}

impl EventReader {
    fn new() -> Self {
        Self {
            buffer: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn start(self, receiver: Receiver<Arc<Event>>) -> Self {
        let buffer = Arc::clone(&self.buffer);
        thread::spawn(move || {
            while let Ok(event) = receiver.recv() {
                if let Ok(mut buffer) = buffer.lock() {
                    buffer.push(event);
                }
            }
        });
        self
    }

    fn take(&self) -> Vec<Arc<Event>> {
        self.buffer
            .lock()
            .map(|mut buffer| std::mem::take(&mut *buffer))
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum EventFilter {
    Duplicates,
    SessionId,
    Folder,
    ExitCodeSuccess,
}

impl Display for EventFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        match self {
            EventFilter::Duplicates => write!(f, "duplicates"),
            EventFilter::SessionId => write!(f, "session id"),
            EventFilter::Folder => write!(f, "folder"),
            EventFilter::ExitCodeSuccess => write!(f, "exit code success"),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct ParseEventFilterError(String);

impl std::fmt::Display for ParseEventFilterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid event filter: {}", self.0)
    }
}

impl std::error::Error for ParseEventFilterError {}

impl FromStr for EventFilter {
    type Err = ParseEventFilterError;
    fn from_str(filter: &str) -> std::result::Result<Self, Self::Err> {
        match filter {
            "duplicates" => Ok(EventFilter::Duplicates),
            "session_id" => Ok(EventFilter::SessionId),
            "folder" => Ok(EventFilter::Folder),
            "exit_code_success" => Ok(EventFilter::ExitCodeSuccess),
            _ => Err(ParseEventFilterError(filter.to_string())),
        }
    }
}

/// View after filtering Events
struct FilteredView<'a> {
    events: &'a [Arc<Event>],
    indices: Vec<usize>,
}

impl<'a> FilteredView<'a> {
    fn build(
        events: &'a [Arc<Event>],
        filters: &HashSet<EventFilter>,
        folder: &str,
        session_id: Option<&str>,
        dedup_map: &HashMap<String, usize>,
    ) -> Self {
        let indices = if filters.contains(&EventFilter::Duplicates) {
            let mut dedup_indices: Vec<usize> = dedup_map.values().copied().collect();
            dedup_indices.sort_unstable();
            dedup_indices
        } else {
            (0..events.len()).collect()
        };

        let indices = indices
            .into_iter()
            .filter(|&i| {
                #[expect(
                    clippy::indexing_slicing,
                    reason = "invariant by construction: i < self.events.len()"
                )]
                let event = &events[i];
                filters.iter().all(|f| match f {
                    EventFilter::Duplicates => true,
                    EventFilter::SessionId => session_id.is_none_or(|sid| event.session == sid),
                    EventFilter::Folder => event.folder == folder,
                    EventFilter::ExitCodeSuccess => event.exit_code == 0,
                })
            })
            .collect();

        Self { events, indices }
    }

    fn entries(&self) -> impl Iterator<Item = (usize, &str)> {
        #[expect(
            clippy::indexing_slicing,
            reason = "invariant by construction: i < self.events.len()"
        )]
        self.indices
            .iter()
            .map(|&i| (i, self.events[i].command.as_str()))
    }
}

pub struct Tui;

impl Tui {
    /// Set up the terminal and run the TUI. `receiver` is fed [`Event`]s by the caller.
    /// Returns the selected event, if any.
    pub fn start(
        receiver: Receiver<Arc<Event>>,
        query: &str,
        folder: &str,
        session_id: Option<String>,
        filters: HashSet<EventFilter>,
        show_score: bool,
    ) -> Option<Event> {
        let reader = EventReader::new().start(receiver);
        Tui::setup_terminal()
            .and_then(|mut terminal| {
                let result = App::new(
                    reader,
                    query.to_string(),
                    folder.to_string(),
                    session_id,
                    filters,
                    show_score,
                )
                .run(&mut terminal);
                Tui::restore_terminal(&mut terminal)?;
                result
            })
            .unwrap_or_default()
    }

    fn setup_terminal() -> anyhow::Result<Terminal<CrosstermBackend<File>>> {
        let mut tty = File::options().read(true).write(true).open("/dev/tty")?;
        enable_raw_mode()?;
        tty.execute(EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(tty);
        let terminal = Terminal::new(backend)?;
        Ok(terminal)
    }

    fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<File>>) -> anyhow::Result<()> {
        terminal.backend_mut().execute(LeaveAlternateScreen)?;
        terminal.show_cursor()?;
        disable_raw_mode()?;
        terminal.backend_mut().flush()?;
        Ok(())
    }
}

/// app holds the state of the application
struct App {
    /// current value of the input box
    input: String,
    /// position of cursor in the editor area.
    character_index: usize,
    /// indices into events sorted according to fuzzy score if we have a query
    indexer: Option<FuzzyIndex>,
    /// reader for collecting events from background thread
    reader: EventReader,
    /// accumulated events pool for filtering and matching
    events: Vec<Arc<Event>>,
    /// currently selected index in the history widget (0 = bottom-most)
    selected_index: usize,
    /// currently active event filter
    filters: HashSet<EventFilter>,
    folder: String,
    session_id: Option<String>,
    show_score: bool,
    /// deduplicated list of entries (see [`EventFilter::Duplicates`])
    dedup_map: HashMap<String, usize>,
}

impl App {
    fn new(
        reader: EventReader,
        query: String,
        folder: String,
        session_id: Option<String>,
        filters: HashSet<EventFilter>,
        show_score: bool,
    ) -> Self {
        let character_index = query.len();
        Self {
            input: query,
            indexer: None,
            character_index,
            reader,
            events: Vec::new(),
            selected_index: 0,
            filters,
            folder,
            session_id,
            show_score,
            dedup_map: HashMap::new(),
        }
    }

    fn collect_new_events(&mut self) {
        let mut new_events = self.reader.take();
        let base = self.events.len();
        for (i, e) in new_events.iter().enumerate() {
            self.dedup_map.entry(e.command.clone()).or_insert(base + i);
        }
        self.events.append(&mut new_events);
    }

    fn run_matcher(&mut self) {
        let filtered = FilteredView::build(
            &self.events,
            &self.filters,
            &self.folder,
            self.session_id.as_deref(),
            &self.dedup_map,
        );
        let entries: Vec<(usize, &str)> = filtered.entries().collect();

        if self.input.is_empty() {
            // pass through (no score, no highlights)
            let matches: Vec<Match> = entries
                .iter()
                .map(|&(idx, _)| (idx, 0i64, vec![]))
                .collect();
            self.indexer = Some(FuzzyIndex::from(matches));
        } else {
            let matcher = FuzzyEngine::new(self.input.clone());
            let mut matches = matcher.match_all(&entries);
            matches.sort_unstable_by_key(|(_, score, _)| std::cmp::Reverse(*score));
            self.indexer = Some(FuzzyIndex::from(matches));
        }
        self.selected_index = 0;
    }

    fn move_cursor_left(&mut self) {
        let cursor_moved_left = self.character_index.saturating_sub(1);
        self.character_index = self.clamp_cursor(cursor_moved_left);
    }

    fn move_cursor_right(&mut self) {
        let cursor_moved_right = self.character_index.saturating_add(1);
        self.character_index = self.clamp_cursor(cursor_moved_right);
    }

    fn enter_char(&mut self, new_char: char) {
        let index = self.byte_index();
        self.input.insert(index, new_char);
        self.move_cursor_right();
        self.run_matcher();
    }

    /// returns the byte index for `character_index`, which counts Unicode scalar values not bytes.
    fn byte_index(&self) -> usize {
        self.input
            .char_indices()
            .map(|(i, _)| i)
            .nth(self.character_index)
            .unwrap_or(self.input.len())
    }

    fn delete_char(&mut self) {
        let is_not_cursor_leftmost = self.character_index != 0;
        if is_not_cursor_leftmost {
            // String::remove works on bytes, not chars; reconstruct around the character instead.
            let current_index = self.character_index;
            let from_left_to_current_index = current_index - 1;
            let before_char_to_delete = self.input.chars().take(from_left_to_current_index);
            let after_char_to_delete = self.input.chars().skip(current_index);
            self.input = before_char_to_delete.chain(after_char_to_delete).collect();
            self.move_cursor_left();
        }

        self.run_matcher();
    }

    /// This mimics ctrl-w found in most terms
    fn delete_word(&mut self) {
        if self.character_index == 0 {
            return;
        }

        let byte_idx = self.byte_index();
        let before_cursor = &self.input[..byte_idx];

        // Trim trailing whitespace, then find last whitespace (word boundary)
        let trimmed = before_cursor.trim_end_matches(|c: char| c.is_whitespace());
        let word_start_byte = trimmed
            .rfind(char::is_whitespace)
            .map(|i| i + trimmed[i..].chars().next().map_or(0, |c| c.len_utf8()))
            .unwrap_or(0);

        let word_start_char = self.input[..word_start_byte].chars().count();
        let before_word = self.input.chars().take(word_start_char);
        let after_cursor = self.input.chars().skip(self.character_index);
        self.input = before_word.chain(after_cursor).collect();
        self.character_index = self.clamp_cursor(word_start_char);

        self.run_matcher();
    }

    fn move_selection_up(&mut self, available_height: usize) {
        let max_index = available_height.saturating_sub(3);
        self.selected_index = (self.selected_index + 1).min(max_index);
    }

    fn move_selection_down(&mut self) {
        self.selected_index = self.selected_index.saturating_sub(1);
    }

    fn clamp_cursor(&self, new_cursor_pos: usize) -> usize {
        new_cursor_pos.clamp(0, self.input.chars().count())
    }

    fn toggle_filter(&mut self, event: EventFilter) {
        if self.filters.contains(&event) {
            self.filters.remove(&event);
        } else {
            self.filters.insert(event);
        }
    }

    fn active_filters(&self) -> String {
        // TODO find a nicer way to visualise the active filters
        self.filters
            .iter()
            .map(|filter| match filter {
                EventFilter::Duplicates => "U".to_string(),
                EventFilter::SessionId => "S".to_string(),
                EventFilter::Folder => "F".to_string(),
                EventFilter::ExitCodeSuccess => "E".to_string(),
            })
            .collect::<Vec<_>>()
            .join(" | ")
    }

    fn run(
        mut self,
        terminal: &mut Terminal<CrosstermBackend<File>>,
    ) -> anyhow::Result<Option<Event>> {
        self.collect_new_events();
        terminal.draw(|frame| self.render(frame))?;

        loop {
            if event::poll(Duration::from_millis(100))? {
                match event::read()? {
                    event::Event::Resize(_width, _height) => {
                        terminal.draw(|frame| self.render(frame))?;
                    }
                    event::Event::Key(key) => {
                        match (key.code, key.modifiers) {
                            (KeyCode::Enter, _) => {
                                let indexer = if let Some(indexer) = self.indexer {
                                    indexer
                                } else {
                                    return Ok(None);
                                };
                                let idx = indexer.get(self.selected_index).ok_or(anyhow!(
                                    "index {:?} not in indexer",
                                    self.selected_index
                                ))?;
                                if let Some(event) = self.events.get(idx) {
                                    let event = Arc::unwrap_or_clone(event.clone());
                                    return Ok(Some(event));
                                }
                                return Ok(None);
                            }
                            (KeyCode::Char(to_insert), KeyModifiers::NONE)
                            | (KeyCode::Char(to_insert), KeyModifiers::SHIFT) => {
                                self.enter_char(to_insert)
                            }
                            (KeyCode::Char('u'), KeyModifiers::CONTROL) => {
                                self.toggle_filter(EventFilter::Duplicates);
                                self.run_matcher();
                            }
                            (KeyCode::Char('s'), KeyModifiers::CONTROL) => {
                                self.toggle_filter(EventFilter::SessionId);
                                self.run_matcher();
                            }
                            (KeyCode::Char('f'), KeyModifiers::CONTROL) => {
                                self.toggle_filter(EventFilter::Folder);
                                self.run_matcher();
                            }
                            (KeyCode::Char('e'), KeyModifiers::CONTROL) => {
                                self.toggle_filter(EventFilter::ExitCodeSuccess);
                                self.run_matcher();
                            }
                            (KeyCode::Char('x'), KeyModifiers::CONTROL) => {
                                self.show_score = !self.show_score;
                                self.run_matcher();
                            }
                            (KeyCode::Char('w'), KeyModifiers::CONTROL) => {
                                self.delete_word();
                            }
                            (KeyCode::Backspace, _) => self.delete_char(),
                            (KeyCode::Left, _) => self.move_cursor_left(),
                            (KeyCode::Right, _) => self.move_cursor_right(),
                            (KeyCode::Up, _) => {
                                let available_height =
                                    terminal.size()?.height.saturating_sub(5) as usize;
                                self.move_selection_up(available_height);
                            }
                            (KeyCode::Down, _) => self.move_selection_down(),
                            (KeyCode::Esc, _)
                            | (KeyCode::Char('c'), KeyModifiers::CONTROL)
                            | (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                                return Ok(None);
                            }
                            _ => {}
                        }
                        terminal.draw(|frame| self.render(frame))?;
                    }
                    // TODO focus gained/lost?
                    _ => {}
                }
            } else {
                let events_before = self.events.len();
                self.collect_new_events();
                if self.events.len() != events_before {
                    self.run_matcher();
                    terminal.draw(|frame| self.render(frame))?;
                }
            }
        }
    }

    fn render_history(
        &self,
        indexer: &FuzzyIndex,
        num_items: usize,
        timeago_fn: impl Fn(&Event) -> String,
    ) -> Vec<ListItem<'_>> {
        indexer
            .first_n(num_items)
            .enumerate()
            .rev()
            .filter_map(|(i, idx)| {
                // TODO should always be Some(...): skip, report, log otherwise?
                let event = self.events.get(idx)?;
                let ago = timeago_fn(event);
                let command = &event.command;
                let mut spans = Vec::new();
                spans.push(Span::raw(format!("{ago} -- ")));
                if let Some(hl_indides) = indexer.highlight_indices(i) {
                    let mut last_index = 0;

                    for &char_index in hl_indides {
                        if char_index > last_index {
                            let text: String = command
                                .chars()
                                .skip(last_index)
                                .take(char_index - last_index)
                                .collect();
                            spans.push(Span::raw(text));
                        }

                        if let Some(char_to_highlight) = command.chars().nth(char_index) {
                            spans.push(Span::styled(
                                char_to_highlight.to_string(),
                                Style::default().fg(Color::Yellow),
                            ));
                            last_index = char_index + 1;
                        }
                    }

                    if last_index < command.len() {
                        let text: String = command.chars().skip(last_index).collect();
                        spans.push(Span::raw(text));
                    }
                } else {
                    spans.push(Span::raw(command.as_str()));
                }

                if self.show_score
                    && let Some(score) = indexer.matcher_score(i)
                {
                    spans.push(Span::raw(format!(" ({})", score)));
                }

                let item = ListItem::new(Line::from(spans));
                if i == self.selected_index {
                    Some(item.style(Style::default().bg(Color::DarkGray)))
                } else {
                    Some(item)
                }
            })
            .collect()
    }

    fn render(&self, frame: &mut Frame) {
        let layout = Layout::vertical([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(5),
        ]);
        let [history_area, status_area, input_area, preview_area] = frame.area().layout(&layout);

        let available_height = history_area.height.saturating_sub(1) as usize;

        let now = Utc::now().timestamp_millis();
        let timeago_fmt = timeago::Formatter::new();
        let timeago_fn = |event: &Event| {
            timeago_fmt.convert(std::time::Duration::from_millis(
                (now - event.endtime) as u64,
            ))
        };
        let history: Vec<ListItem> = if let Some(indexer) = &self.indexer {
            self.render_history(indexer, available_height.min(self.events.len()), timeago_fn)
        } else {
            vec![]
        };
        let history_widget = List::new(history)
            .block(Block::default().padding(ratatui::widgets::Padding::horizontal(2)));
        frame.render_widget(history_widget, history_area);

        let filtered = if let Some(indexer) = &self.indexer {
            indexer.len()
        } else {
            0
        };
        let status_text = format!("{filtered}/{}", self.events.len());
        let status_line = Line::from(vec![Span::raw("  "), Span::raw(status_text)]);
        let filters = format!("[{}]  ", self.active_filters());
        let status_line_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(0), Constraint::Length(filters.len() as u16)])
            .split(status_area);
        #[expect(clippy::indexing_slicing, reason = "constructed two constraints")]
        frame.render_widget(status_line, status_line_chunks[0]);
        #[expect(clippy::indexing_slicing, reason = "constructed two constraints")]
        frame.render_widget(filters, status_line_chunks[1]);

        let input_line = Line::from(vec![
            Span::raw("> "),
            Span::styled(
                self.input.as_str(),
                Style::default().fg(Color::Yellow).not_bold(),
            ),
        ]);
        let input = Paragraph::new(input_line).block(Block::default());
        frame.render_widget(input, input_area);
        frame.set_cursor_position(Position::new(
            input_area.x + self.character_index as u16 + 2,
            input_area.y,
        ));

        let preview_content = if let Some(indexer) = &self.indexer
            && let Some(idx) = indexer.get(self.selected_index)
        {
            if let Some(event) = self.events.get(idx) {
                format!("[exit code={}]: {}", event.exit_code, event.command)
            } else {
                String::new()
            }
        } else {
            String::new()
        };
        let preview = Paragraph::new(preview_content)
            .block(
                Block::default()
                    .borders(ratatui::widgets::Borders::TOP)
                    .border_style(Style::default().fg(Color::DarkGray)),
            )
            .wrap(ratatui::widgets::Wrap { trim: false });
        frame.render_widget(preview, preview_area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_app(input: &str) -> App {
        let character_index = input.chars().count();
        App {
            input: input.to_string(),
            character_index,
            indexer: None,
            reader: EventReader::new(),
            events: Vec::new(),
            selected_index: 0,
            filters: HashSet::new(),
            folder: String::new(),
            session_id: None,
            show_score: false,
            dedup_map: HashMap::new(),
        }
    }

    #[test]
    fn delete_word_basic() {
        let mut app = make_app("hello world");
        app.delete_word();
        assert_eq!(app.input, "hello ");
        assert_eq!(app.character_index, 6);
    }

    #[test]
    fn delete_word_trailing_spaces() {
        let mut app = make_app("hello   ");
        app.delete_word();
        assert_eq!(app.input, "");
        assert_eq!(app.character_index, 0);
    }

    #[test]
    fn delete_word_single_word() {
        let mut app = make_app("hello");
        app.delete_word();
        assert_eq!(app.input, "");
        assert_eq!(app.character_index, 0);
    }

    #[test]
    fn delete_word_at_start_is_noop() {
        let mut app = make_app("hello");
        app.character_index = 0;
        app.delete_word();
        assert_eq!(app.input, "hello");
        assert_eq!(app.character_index, 0);
    }

    #[test]
    fn delete_word_mid_word() {
        let mut app = make_app("hello world");
        app.character_index = 7; // cursor between 'w' and 'o' in "world"
        app.delete_word();
        assert_eq!(app.input, "hello orld");
        assert_eq!(app.character_index, 6);
    }

    #[test]
    fn byte_index_ascii() {
        let app = make_app("hello");
        assert_eq!(app.byte_index(), 5);
    }

    #[test]
    fn byte_index_multibyte() {
        // "é" is 2 bytes; cursor after it should give byte index 2
        let mut app = make_app("é");
        app.character_index = 1;
        assert_eq!(app.byte_index(), 2);
    }

    #[test]
    fn byte_index_at_start() {
        let mut app = make_app("hello");
        app.character_index = 0;
        assert_eq!(app.byte_index(), 0);
    }

    #[test]
    fn toggle_filter_adds_and_removes() {
        let mut app = make_app("");
        app.toggle_filter(EventFilter::Duplicates);
        assert!(app.filters.contains(&EventFilter::Duplicates));
        app.toggle_filter(EventFilter::Duplicates);
        assert!(!app.filters.contains(&EventFilter::Duplicates));
    }

    #[test]
    fn active_filters_empty() {
        let app = make_app("");
        assert_eq!(app.active_filters(), "");
    }

    #[test]
    fn active_filters_shows_abbreviations() {
        let mut app = make_app("");
        app.toggle_filter(EventFilter::Duplicates);
        app.toggle_filter(EventFilter::ExitCodeSuccess);
        app.toggle_filter(EventFilter::Folder);
        app.toggle_filter(EventFilter::SessionId);
        let filters = app.active_filters();
        assert!(filters.contains('U'));
        assert!(filters.contains('E'));
        assert!(filters.contains('F'));
        assert!(filters.contains('S'));
    }

    #[test]
    fn move_selection_up_increments() {
        let mut app = make_app("");
        app.move_selection_up(10);
        assert_eq!(app.selected_index, 1);
    }

    #[test]
    fn move_selection_up_clamps_to_height() {
        let mut app = make_app("");
        app.selected_index = 6;
        app.move_selection_up(8); // max = 8 - 3 = 5
        assert_eq!(app.selected_index, 5);
    }

    #[test]
    fn move_selection_down_decrements() {
        let mut app = make_app("");
        app.selected_index = 3;
        app.move_selection_down();
        assert_eq!(app.selected_index, 2);
    }

    #[test]
    fn move_selection_down_clamps_at_zero() {
        let mut app = make_app("");
        app.move_selection_down();
        assert_eq!(app.selected_index, 0);
    }

    #[test]
    fn enter_char_appends_at_end() {
        let mut app = make_app("hell");
        app.enter_char('o');
        assert_eq!(app.input, "hello");
        assert_eq!(app.character_index, 5);
    }

    #[test]
    fn enter_char_inserts_at_start() {
        let mut app = make_app("ello");
        app.character_index = 0;
        app.enter_char('h');
        assert_eq!(app.input, "hello");
        assert_eq!(app.character_index, 1);
    }

    #[test]
    fn enter_char_inserts_in_middle() {
        let mut app = make_app("hllo");
        app.character_index = 1;
        app.enter_char('e');
        assert_eq!(app.input, "hello");
        assert_eq!(app.character_index, 2);
    }

    #[test]
    fn enter_char_multibyte() {
        let mut app = make_app("h");
        app.enter_char('é');
        assert_eq!(app.input, "hé");
        assert_eq!(app.character_index, 2);
    }

    #[test]
    fn collect_new_events_drains_channel() {
        let (sender, receiver) = crossbeam_channel::unbounded();
        let mut app = make_app("");
        app.reader = EventReader::new().start(receiver);

        let event = Arc::new(Event {
            timestamp_millis: 0,
            command: "git status".to_string(),
            endtime: 1000,
            exit_code: 0,
            folder: "/".to_string(),
            machine: "m".to_string(),
            session: "s".to_string(),
        });
        sender.send(event).unwrap();
        drop(sender); // closing the channel lets us wait for the thread to drain it
        std::thread::sleep(std::time::Duration::from_millis(10));

        app.collect_new_events();
        assert_eq!(app.events.len(), 1);
        assert_eq!(app.events[0].command, "git status");
    }

    #[test]
    fn move_cursor_left_decrements() {
        let mut app = make_app("hello");
        app.move_cursor_left();
        assert_eq!(app.character_index, 4);
    }

    #[test]
    fn move_cursor_left_clamps_at_zero() {
        let mut app = make_app("hello");
        app.character_index = 0;
        app.move_cursor_left();
        assert_eq!(app.character_index, 0);
    }

    #[test]
    fn move_cursor_right_increments() {
        let mut app = make_app("hello");
        app.character_index = 0;
        app.move_cursor_right();
        assert_eq!(app.character_index, 1);
    }

    #[test]
    fn move_cursor_right_clamps_at_end() {
        let mut app = make_app("hello");
        app.move_cursor_right();
        assert_eq!(app.character_index, 5);
    }

    #[test]
    fn delete_char_basic() {
        let mut app = make_app("hello");
        app.delete_char();
        assert_eq!(app.input, "hell");
        assert_eq!(app.character_index, 4);
    }

    #[test]
    fn delete_char_at_start_is_noop() {
        let mut app = make_app("hello");
        app.character_index = 0;
        app.delete_char();
        assert_eq!(app.input, "hello");
        assert_eq!(app.character_index, 0);
    }

    #[test]
    fn delete_char_multibyte() {
        let mut app = make_app("héllo");
        app.character_index = 2; // cursor after 'é'
        app.delete_char();
        assert_eq!(app.input, "hllo");
        assert_eq!(app.character_index, 1);
    }

    #[test]
    fn event_filter_from_str_roundtrip() {
        let cases = [
            ("duplicates", EventFilter::Duplicates),
            ("session_id", EventFilter::SessionId),
            ("folder", EventFilter::Folder),
            ("exit_code_success", EventFilter::ExitCodeSuccess),
        ];
        for (input, expected) in cases {
            assert_eq!(input.parse::<EventFilter>().unwrap(), expected);
        }
    }

    #[test]
    fn event_filter_from_str_unknown() {
        assert!("unknown".parse::<EventFilter>().is_err());
    }
}
