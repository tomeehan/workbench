use std::time::Duration;

use color_eyre::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};

use crate::db::{Database, Project, Session, Status};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    NewSession,
}

pub struct App {
    pub should_quit: bool,
    pub db: Database,
    pub project: Project,
    pub sessions: Vec<Session>,
    pub selected_column: usize,
    pub selected_row: usize,
    pub input_mode: InputMode,
    pub input_buffer: String,
}

impl App {
    pub fn new() -> Result<Self> {
        let db = Database::new()?;
        let cwd = std::env::current_dir()?;
        let project_name = cwd
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown");
        let project_path = cwd.to_string_lossy().to_string();

        let project = db.get_or_create_project(project_name, &project_path)?;
        let sessions = db.list_sessions(project.id)?;

        Ok(Self {
            should_quit: false,
            db,
            project,
            sessions,
            selected_column: 0,
            selected_row: 0,
            input_mode: InputMode::Normal,
            input_buffer: String::new(),
        })
    }

    pub fn sessions_by_status(&self, status: Status) -> Vec<&Session> {
        self.sessions
            .iter()
            .filter(|s| s.status == status)
            .collect()
    }

    pub fn selected_session(&self) -> Option<&Session> {
        let status = Status::all().get(self.selected_column)?;
        let sessions = self.sessions_by_status(*status);
        sessions.get(self.selected_row).copied()
    }

    pub fn refresh_sessions(&mut self) -> Result<()> {
        self.sessions = self.db.list_sessions(self.project.id)?;
        Ok(())
    }

    pub fn handle_events(&mut self) -> Result<()> {
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match self.input_mode {
                    InputMode::Normal => self.handle_normal_key(key)?,
                    InputMode::NewSession => self.handle_input_key(key)?,
                }
            }
        }
        Ok(())
    }

    fn handle_normal_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true
            }
            KeyCode::Char('n') => {
                self.input_mode = InputMode::NewSession;
                self.input_buffer.clear();
            }
            KeyCode::Char('h') | KeyCode::Left => {
                if self.selected_column > 0 {
                    self.selected_column -= 1;
                    self.clamp_row();
                }
            }
            KeyCode::Char('l') | KeyCode::Right => {
                if self.selected_column < Status::all().len() - 1 {
                    self.selected_column += 1;
                    self.clamp_row();
                }
            }
            KeyCode::Char('j') | KeyCode::Down => {
                let status = Status::all()[self.selected_column];
                let count = self.sessions_by_status(status).len();
                if self.selected_row < count.saturating_sub(1) {
                    self.selected_row += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.selected_row > 0 {
                    self.selected_row -= 1;
                }
            }
            KeyCode::Char('m') => {
                if let Some(session) = self.selected_session() {
                    let session_id = session.id;
                    let current_status = session.status;
                    let statuses = Status::all();
                    let current_idx = statuses.iter().position(|s| *s == current_status).unwrap_or(0);
                    let next_idx = (current_idx + 1) % statuses.len();
                    let new_status = statuses[next_idx];
                    self.db.update_session_status(session_id, new_status)?;
                    self.refresh_sessions()?;
                }
            }
            KeyCode::Char('d') => {
                if let Some(session) = self.selected_session() {
                    let session_id = session.id;
                    self.db.delete_session(session_id)?;
                    self.refresh_sessions()?;
                    self.clamp_row();
                }
            }
            KeyCode::Char('r') => {
                self.refresh_sessions()?;
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_input_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
                self.input_buffer.clear();
            }
            KeyCode::Enter => {
                if !self.input_buffer.is_empty() {
                    self.db.create_session(self.project.id, &self.input_buffer)?;
                    self.refresh_sessions()?;
                }
                self.input_mode = InputMode::Normal;
                self.input_buffer.clear();
            }
            KeyCode::Backspace => {
                self.input_buffer.pop();
            }
            KeyCode::Char(c) => {
                self.input_buffer.push(c);
            }
            _ => {}
        }
        Ok(())
    }

    fn clamp_row(&mut self) {
        let status = Status::all()[self.selected_column];
        let count = self.sessions_by_status(status).len();
        if count == 0 {
            self.selected_row = 0;
        } else if self.selected_row >= count {
            self.selected_row = count - 1;
        }
    }
}
