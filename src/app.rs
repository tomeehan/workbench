use std::collections::HashSet;
use std::time::Duration;

use color_eyre::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};

use crate::db::{Database, Project, Session, Status};
use crate::tmux;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    NewSession,
    RenameSession,
}

#[derive(Debug, Clone)]
pub enum AppAction {
    None,
    AttachTmux(String),
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
    pub active_tmux_sessions: HashSet<String>,
    pub sessions_waiting_input: HashSet<String>,
    pub editing_session_id: Option<i64>,
    pub peek_active: bool,
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
        let active_tmux_sessions: HashSet<String> = tmux::list_workbench_sessions().into_iter().collect();

        // Check which sessions are waiting for user input
        let sessions_waiting_input: HashSet<String> = active_tmux_sessions
            .iter()
            .filter(|name| tmux::is_waiting_for_input(name))
            .cloned()
            .collect();

        Ok(Self {
            should_quit: false,
            db,
            project,
            sessions,
            selected_column: 0,
            selected_row: 0,
            input_mode: InputMode::Normal,
            input_buffer: String::new(),
            active_tmux_sessions,
            sessions_waiting_input,
            editing_session_id: None,
            peek_active: false,
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
        self.refresh_tmux_sessions();
        Ok(())
    }

    pub fn refresh_tmux_sessions(&mut self) {
        self.active_tmux_sessions = tmux::list_workbench_sessions().into_iter().collect();

        // Check which sessions are waiting for user input
        self.sessions_waiting_input.clear();
        for name in &self.active_tmux_sessions {
            if tmux::is_waiting_for_input(name) {
                self.sessions_waiting_input.insert(name.clone());
            }
        }

        // Clean up stale tmux references in the database
        for session in &self.sessions {
            if let Some(ref tmux_name) = session.tmux_window {
                if !self.active_tmux_sessions.contains(tmux_name) {
                    let _ = self.db.clear_tmux_session(session.id);
                }
            }
        }
    }

    pub fn has_active_terminal(&self, session: &Session) -> bool {
        session.tmux_window.as_ref()
            .map(|name| self.active_tmux_sessions.contains(name))
            .unwrap_or(false)
    }

    pub fn is_waiting_for_input(&self, session: &Session) -> bool {
        session.tmux_window.as_ref()
            .map(|name| self.sessions_waiting_input.contains(name))
            .unwrap_or(false)
    }

    pub fn handle_events(&mut self) -> Result<AppAction> {
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match self.input_mode {
                    InputMode::Normal => return self.handle_normal_key(key),
                    InputMode::NewSession => self.handle_input_key(key)?,
                    InputMode::RenameSession => self.handle_rename_key(key)?,
                }
            }
        }
        Ok(AppAction::None)
    }

    fn handle_normal_key(&mut self, key: KeyEvent) -> Result<AppAction> {
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
            KeyCode::Char('e') => {
                if let Some(session) = self.selected_session() {
                    let session_id = session.id;
                    let session_name = session.name.clone();
                    self.editing_session_id = Some(session_id);
                    self.input_buffer = session_name;
                    self.input_mode = InputMode::RenameSession;
                }
            }
            KeyCode::Enter => {
                return self.handle_enter_key();
            }
            KeyCode::Char(' ') => {
                if self.selected_session().and_then(|s| s.tmux_window.as_ref()).is_some() {
                    self.peek_active = !self.peek_active;
                }
            }
            _ => {}
        }
        Ok(AppAction::None)
    }

    fn handle_enter_key(&mut self) -> Result<AppAction> {
        if !tmux::is_available() {
            // tmux not installed, do nothing
            return Ok(AppAction::None);
        }

        let Some(session) = self.selected_session() else {
            return Ok(AppAction::None);
        };

        let session_id = session.id;
        let tmux_name = tmux::session_name(self.project.id, session_id);

        // Check if tmux session already exists
        if tmux::session_exists(&tmux_name) {
            return Ok(AppAction::AttachTmux(tmux_name));
        }

        // Create a new tmux session
        tmux::create_session(&tmux_name, &self.project.path)?;
        self.db.set_tmux_session(session_id, &tmux_name)?;
        self.active_tmux_sessions.insert(tmux_name.clone());

        Ok(AppAction::AttachTmux(tmux_name))
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

    fn handle_rename_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
                self.input_buffer.clear();
                self.editing_session_id = None;
            }
            KeyCode::Enter => {
                if !self.input_buffer.is_empty() {
                    if let Some(session_id) = self.editing_session_id {
                        self.db.update_session_name(session_id, &self.input_buffer)?;
                        self.refresh_sessions()?;
                    }
                }
                self.input_mode = InputMode::Normal;
                self.input_buffer.clear();
                self.editing_session_id = None;
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
