use std::{
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use chrono::Utc;
use crossbeam_channel::Receiver;
use crossterm::event::{self, KeyCode};
use ratatui::{
    DefaultTerminal, Frame,
    layout::{Constraint, Layout, Position},
    style::{Color, Style, Stylize},
    text::{Line, Span, Text},
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

        pub fn match_line(&self, line: &str) -> i64 {
            if let Some((score, _indices)) = self.matcher.fuzzy_indices(line, &self.query) {
                // TODO return indices for nicer vis
                return score;
            }

            0
        }
    }
}

struct Reader {
    buffer: Arc<Mutex<Vec<Arc<Event>>>>,
}

impl Reader {
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

pub fn ui(receiver: Receiver<Arc<Event>>) {
    let reader = Reader::new().start(receiver);
    ratatui::run(|terminal| App::new(reader).run(terminal)).unwrap();
}

/// App holds the state of the application
struct App {
    /// Current value of the input box
    input: String,
    /// Position of cursor in the editor area.
    character_index: usize,
    /// History of recorded messages
    history: Vec<String>,
    /// Reader for collecting events from background thread
    reader: Reader,
    /// Accumulated events pool for filtering and matching
    events: Vec<Arc<Event>>,
    indices: Option<Vec<usize>>,
}

impl App {
    fn new(reader: Reader) -> Self {
        Self {
            input: String::new(),
            history: Vec::new(),
            character_index: 0,
            reader,
            events: Vec::new(),
            indices: None,
        }
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

        // TODO only if query is not empty but here we only trigger after keypress so okay
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

        if self.input.is_empty() {
            self.indices = None;
        } else {
            self.run_matcher();
        }
    }

    fn run_matcher(&mut self) {
        // TODO launch matcher - maybe not on every keypress and/or cancle running
        let matcher = fuzzer::FuzzyEngine::new(self.input.clone());

        // TODO parallel matching inside fuzzy engine?
        let scores: Vec<i64> = self
            .events
            .par_iter()
            .map(|x| matcher.match_line(&x.command))
            .collect();

        let mut indices: Vec<usize> = (0..self.events.len()).filter(|&i| scores[i] > 0).collect();
        indices.sort_by_key(|&i| std::cmp::Reverse(scores[i]));
        self.indices = Some(indices);
    }

    fn clamp_cursor(&self, new_cursor_pos: usize) -> usize {
        new_cursor_pos.clamp(0, self.input.chars().count())
    }

    fn collect_new_events(&mut self) {
        let mut new_events = self.reader.take();
        self.events.append(&mut new_events);
    }

    fn update_display(&mut self) {
        self.history.clear();
        let f = timeago::Formatter::new();
        for event in &self.events {
            let ago = f.convert_chrono(event.endtime(), Utc::now());
            let pretty = format!("{ago} --- {}", event.command);
            self.history.push(pretty);
        }
    }

    fn accept(&self) {
        let idx = if let Some(indices) = &self.indices {
            *indices.first().unwrap()
        } else {
            0
        };
        println!("{}", self.events[idx].command);
    }

    fn run(mut self, terminal: &mut DefaultTerminal) -> anyhow::Result<()> {
        self.collect_new_events();
        self.update_display();
        terminal.draw(|frame| self.render(frame))?;

        loop {
            if event::poll(Duration::from_millis(100))? {
                if let Some(key) = event::read()?.as_key_press_event() {
                    match key.code {
                        KeyCode::Enter => {
                            self.accept();
                            return Ok(());
                        }
                        KeyCode::Char(to_insert) => self.enter_char(to_insert),
                        KeyCode::Backspace => self.delete_char(),
                        KeyCode::Left => self.move_cursor_left(),
                        KeyCode::Right => self.move_cursor_right(),
                        KeyCode::Esc => return Ok(()),
                        _ => {}
                    }
                    terminal.draw(|frame| self.render(frame))?;
                }
            } else {
                let events_before = self.events.len();
                self.collect_new_events();
                if self.events.len() != events_before {
                    self.update_display();
                    terminal.draw(|frame| self.render(frame))?;
                }
            }
        }
    }

    fn render(&self, frame: &mut Frame) {
        let layout = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(3),
        ]);
        let [title_area, history_area, status_area, input_area] = frame.area().layout(&layout);

        let (msg, style) = (vec!["osh-oxy".bold()], Style::default());
        let text = Text::from(Line::from(msg)).patch_style(style);
        let title = Paragraph::new(text);
        frame.render_widget(title, title_area);

        let input = Paragraph::new(self.input.as_str())
            .style(Style::default().fg(Color::Yellow).not_bold())
            .block(Block::bordered().title(" search "));
        frame.render_widget(input, input_area);
        frame.set_cursor_position(Position::new(
            // Draw the cursor at the current position in the input field.
            // This position can be controlled via the left and right arrow key
            input_area.x + self.character_index as u16 + 1,
            // Move one line down, from the border to the input line
            input_area.y + 1,
        ));

        // TODO matcher tells us how many are left
        let filtered = if let Some(indices) = &self.indices {
            indices.len()
        } else {
            self.events.len()
        };
        let status_text = format!("{filtered} / {}", self.history.len());
        let status = Paragraph::new(status_text).style(Style::default().fg(Color::Cyan));
        frame.render_widget(status, status_area);

        let available_height = history_area.height.saturating_sub(2) as usize;
        let history = if let Some(indices) = &self.indices {
            let history: Vec<ListItem> = indices[0..available_height]
                .iter()
                .rev()
                .map(|m| {
                    let content = Line::from(Span::raw(self.history[*m].clone()));
                    ListItem::new(content)
                })
                .collect();
            history
        } else {
            let history: Vec<ListItem> = self.history[0..available_height]
                .iter()
                .rev()
                .map(|m| {
                    let content = Line::from(Span::raw(m));
                    ListItem::new(content)
                })
                .collect();
            history
        };

        let history_widget = List::new(history).block(Block::bordered());
        frame.render_widget(history_widget, history_area);

        // TODO preview
    }
}
