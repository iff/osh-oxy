use std::{
    fmt::Display,
    fs::File,
    io::Write,
    iter::Copied,
    ops::Range,
    slice::Iter,
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
use itertools::Either;
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Layout, Position},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, List, ListItem, Paragraph},
};
use rayon::prelude::*;

use crate::event::Event;

mod fuzzer {
    use fuzzy_matcher::{FuzzyMatcher, skim::SkimMatcherV2};
    const BYTES_1M: usize = 1024 * 1024 * 1024;

    pub struct FuzzyEngine {
        query: String,
        matcher: SkimMatcherV2,
    }

    impl FuzzyEngine {
        pub fn new(query: String) -> Self {
            let matcher = SkimMatcherV2::default().element_limit(BYTES_1M);
            let matcher = matcher.smart_case();
            FuzzyEngine { matcher, query }
        }

        pub fn match_line(&self, line: &str) -> (i64, Vec<usize>) {
            if let Some((score, indices)) = self.matcher.fuzzy_indices(line, &self.query) {
                return (score, indices);
            }

            (0, vec![])
        }
    }
}

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

#[derive(Debug, Clone)]
pub enum EventFilter {
    Duplicates,
    SessionId,
    Folder,
}

impl Display for EventFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        match self {
            EventFilter::Duplicates => write!(f, "duplicates"),
            EventFilter::SessionId => write!(f, "session id"),
            EventFilter::Folder => write!(f, "folder"),
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
            _ => Err(ParseEventFilterError(filter.to_string())),
        }
    }
}

pub struct Tui;

impl Tui {
    pub fn start(
        receiver: Receiver<Arc<Event>>,
        query: &str,
        folder: &str,
        session_id: Option<String>,
        filter: Option<EventFilter>,
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
                    filter,
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

// TODO should also own the data?
struct FuzzyIndex {
    /// indices into [`App.history`]
    indices: Option<Vec<usize>>,
    /// scores parallel to indices
    scores: Option<Vec<i64>>,
    /// highlight matches, globally indexed, parallel to [`App.history`]
    highlight_indices: Option<Vec<Vec<usize>>>,
}

impl FuzzyIndex {
    /// creates an identity mapping
    pub fn identity() -> Self {
        Self {
            indices: None,
            scores: None,
            highlight_indices: None,
        }
    }

    /// crates an index from a matcher result
    pub fn new(scored_indices: Vec<(usize, i64)>, highlight_indices: Vec<Vec<usize>>) -> Self {
        let (indices, scores) = scored_indices.into_iter().unzip();
        Self {
            indices: Some(indices),
            scores: Some(scores),
            highlight_indices: Some(highlight_indices),
        }
    }

    /// number of matches or None (if all)
    pub fn len(&self) -> Option<usize> {
        self.indices.as_ref().map(|ind| ind.len())
    }

    /// gets the first n indices
    pub fn first_n(&self, n: usize) -> Either<Copied<Iter<'_, usize>>, Range<usize>> {
        if let Some(indices) = &self.indices {
            let visible_count = n.min(indices.len());
            #[allow(clippy::indexing_slicing)] // slicing: using min ensures the slice is valid
            Either::Left(indices[0..visible_count].iter().copied())
        } else {
            Either::Right(0..n)
        }

        // TODO this should return (index, history_index, highlight_indices, scores)?
    }

    /// get the i-th index
    pub fn get(&self, index: usize) -> Option<usize> {
        if let Some(indices) = &self.indices {
            indices.get(index).copied()
        } else {
            Some(index)
        }
    }

    pub fn matcher_score(&self, index: usize) -> Option<i64> {
        if let Some(scores) = &self.scores {
            scores.get(index).copied()
        } else {
            None
        }
    }

    /// get the highlight indices
    pub fn highlight_indices(&self, index: usize) -> Option<&Vec<usize>> {
        if let Some(indices) = &self.highlight_indices {
            indices.get(index)
        } else {
            None
        }
    }
}

/// App holds the state of the application
struct App {
    /// Current value of the input box
    input: String,
    /// Position of cursor in the editor area.
    character_index: usize,
    /// History of recorded messages
    history: Vec<(String, String)>,
    /// indices into history sorted according to fuzzer score if we have a query
    indexer: FuzzyIndex,
    /// Reader for collecting events from background thread
    reader: EventReader,
    /// Accumulated events pool for filtering and matching
    events: Vec<Arc<Event>>,
    /// Currently selected index in the history widget (0 = bottom-most)
    selected_index: usize,
    /// currently active event filter
    filter: Option<EventFilter>,
    folder: String,
    /// Current session id
    session_id: Option<String>,
    show_score: bool,
}

impl App {
    fn new(
        reader: EventReader,
        query: String,
        folder: String,
        session_id: Option<String>,
        filter: Option<EventFilter>,
        show_score: bool,
    ) -> Self {
        let character_index = query.len();
        Self {
            input: query,
            history: Vec::new(),
            indexer: FuzzyIndex::identity(),
            character_index,
            reader,
            events: Vec::new(),
            selected_index: 0,
            filter,
            folder,
            session_id,
            show_score,
        }
    }

    fn collect_new_events(&mut self) {
        let mut new_events = self.reader.take();
        self.events.append(&mut new_events);
    }

    fn run_matcher(&mut self) {
        if self.input.is_empty() {
            self.indexer = FuzzyIndex::identity();
            return;
        }

        // TODO matcher should go into its own module and accumulate results into a struct before
        // sorting
        let matcher = fuzzer::FuzzyEngine::new(self.input.clone());
        let scores: (Vec<i64>, Vec<Vec<usize>>) = self
            .events
            .par_iter()
            .map(|event| {
                let passes_filter = match &self.filter {
                    None => true,
                    // handle later or maybe keeping those in memory as well
                    Some(EventFilter::Duplicates) => true,
                    Some(EventFilter::SessionId) => self
                        .session_id
                        .as_ref()
                        .is_none_or(|sid| event.session == *sid),
                    Some(EventFilter::Folder) => event.folder == self.folder,
                };

                if passes_filter {
                    matcher.match_line(&event.command)
                } else {
                    (0, vec![])
                }
            })
            .collect();

        let mut scored_indices = match &self.filter {
            Some(EventFilter::Duplicates) => {
                // TODO not so sure, maybe find a better approach here
                use std::collections::HashSet;
                let mut seen = HashSet::new();
                scores
                    .0
                    .into_iter()
                    .enumerate()
                    .filter(|(idx, score)| {
                        if let Some(event) = &self.events.get(*idx) {
                            *score > 0 && seen.insert(&event.command)
                        } else {
                            false
                        }
                    })
                    .collect::<Vec<(usize, i64)>>()
            }
            _ => scores
                .0
                .into_par_iter()
                .enumerate()
                .filter(|(_, score)| *score > 0)
                .collect::<Vec<(usize, i64)>>(),
        };

        scored_indices.sort_by_key(|(_, score)| std::cmp::Reverse(*score));
        self.indexer = FuzzyIndex::new(scored_indices, scores.1);
        // TODO overwrite (or max)?
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

    /// Returns the byte index based on the character position.
    ///
    /// Since each character in a string can contain multiple bytes, it's necessary to calculate
    /// the byte index based on the index of the character.
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

    fn update_display(&mut self) {
        self.history.clear();
        let f = timeago::Formatter::new();
        let now = Utc::now().timestamp_millis();
        for event in &self.events {
            let d = std::time::Duration::from_millis((now - event.endtimestamp()) as u64);
            let ago = f.convert(d);
            // TODO clone
            self.history.push((ago, event.command.clone()));
        }
    }

    fn run(
        mut self,
        terminal: &mut Terminal<CrosstermBackend<File>>,
    ) -> anyhow::Result<Option<Event>> {
        self.collect_new_events();
        self.update_display();
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
                                let idx = self.indexer.get(self.selected_index).ok_or(anyhow!(
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
                            (KeyCode::Tab, _) => {
                                self.filter = match &self.filter {
                                    None => Some(EventFilter::Duplicates),
                                    Some(EventFilter::Duplicates) => Some(EventFilter::SessionId),
                                    Some(EventFilter::SessionId) => Some(EventFilter::Folder),
                                    Some(EventFilter::Folder) => None,
                                };
                                self.run_matcher();
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
                    self.update_display();
                    terminal.draw(|frame| self.render(frame))?;
                }
            }
        }
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

        let history: Vec<ListItem> = self
            .indexer
            .first_n(available_height.min(self.history.len()))
            .enumerate()
            .rev()
            .filter_map(|(i, idx)| {
                // TODO should always be Some(...): skip, report, log otherwise?
                let (ago, command) = self.history.get(idx)?;
                let mut spans = Vec::new();
                spans.push(Span::raw(format!("{ago} -- ")));
                if let Some(hl_indides) = self.indexer.highlight_indices(idx) {
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
                    spans.push(Span::raw(command.clone()));
                }

                if self.show_score
                    && let Some(score) = self.indexer.matcher_score(i)
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
            .collect();
        let history_widget = List::new(history)
            .block(Block::default().padding(ratatui::widgets::Padding::horizontal(2)));
        frame.render_widget(history_widget, history_area);

        let filtered = self.indexer.len().unwrap_or(self.events.len());
        let filter = if let Some(f) = &self.filter {
            format!("filtered {f}")
        } else {
            "no filter".to_string()
        };
        let status_text = format!("{filtered}/{}", self.history.len());
        let status_line = Line::from(vec![
            Span::raw("  "),
            Span::raw(status_text),
            Span::raw(" ["),
            Span::raw(filter),
            Span::raw("]"),
        ]);
        frame.render_widget(status_line, status_area);

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

        let preview_content = if let Some(idx) = self.indexer.get(self.selected_index) {
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
