use std::collections::HashSet;
use std::time::Duration;

use color_eyre::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};

use crate::db::{Database, Field, Project, Session, Status};
use crate::tmux;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum View {
    #[default]
    Kanban,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    NewSession,
    EditSession,
    MoveSession,
    NewFieldName,
    NewFieldDesc,
    EditFieldName,
    EditFieldDesc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EditMode {
    #[default]
    Manual,
    AI,
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
    pub moving_session_id: Option<i64>,
    pub peek_active: bool,
    pub edit_row: usize,
    pub edit_session_name: String,
    pub edit_field_values: Vec<String>,
    pub edit_mode: EditMode,
    pub ai_loading: bool,
    pub view: View,
    pub fields: Vec<Field>,
    pub selected_field: usize,
    pub editing_field_id: Option<i64>,
    pub new_field_name: String,
    pub new_field_desc: String,
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
        let fields = db.list_fields(project.id)?;
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
            moving_session_id: None,
            peek_active: false,
            edit_row: 0,
            edit_session_name: String::new(),
            edit_field_values: Vec::new(),
            edit_mode: EditMode::default(),
            ai_loading: false,
            view: View::default(),
            fields,
            selected_field: 0,
            editing_field_id: None,
            new_field_name: String::new(),
            new_field_desc: String::new(),
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

    pub fn refresh_fields(&mut self) -> Result<()> {
        self.fields = self.db.list_fields(self.project.id)?;
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
                    InputMode::Normal => {
                        match self.view {
                            View::Kanban => return self.handle_normal_key(key),
                            View::Settings => self.handle_settings_key(key)?,
                        }
                    }
                    InputMode::NewSession => self.handle_input_key(key)?,
                    InputMode::EditSession => self.handle_edit_session_key(key)?,
                    InputMode::MoveSession => self.handle_move_key(key)?,
                    InputMode::NewFieldName => self.handle_new_field_name_key(key)?,
                    InputMode::NewFieldDesc => self.handle_new_field_desc_key(key)?,
                    InputMode::EditFieldName => self.handle_edit_field_name_key(key)?,
                    InputMode::EditFieldDesc => self.handle_edit_field_desc_key(key)?,
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
                    self.moving_session_id = Some(session.id);
                    self.input_mode = InputMode::MoveSession;
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
                    self.edit_session_name = session_name.clone();
                    self.edit_row = 0;
                    self.input_buffer = session_name;
                    // Load field values
                    let field_values: Vec<String> = self.fields.iter().map(|f| {
                        self.db.get_session_field_value(session_id, f.id).unwrap_or_default()
                    }).collect();
                    self.edit_field_values = field_values;
                    self.input_mode = InputMode::EditSession;
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
            KeyCode::Char('s') => {
                self.view = View::Settings;
                self.selected_field = 0;
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

    fn handle_edit_session_key(&mut self, key: KeyEvent) -> Result<()> {
        let total_rows = 1 + self.fields.len(); // name + custom fields
        match key.code {
            KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
                self.input_buffer.clear();
                self.editing_session_id = None;
                self.edit_session_name.clear();
                self.edit_field_values.clear();
                self.edit_mode = EditMode::Manual;
            }
            // Shift+Tab cycles between Manual and AI mode
            KeyCode::BackTab if key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.save_current_edit_row();
                self.edit_mode = match self.edit_mode {
                    EditMode::Manual => EditMode::AI,
                    EditMode::AI => EditMode::Manual,
                };
                // If switching to AI mode, run AI fill
                if self.edit_mode == EditMode::AI {
                    self.run_ai_fill();
                }
            }
            KeyCode::Tab | KeyCode::Down => {
                self.save_current_edit_row();
                if self.edit_row < total_rows - 1 {
                    self.edit_row += 1;
                } else {
                    self.edit_row = 0;
                }
                self.load_current_edit_row();
            }
            KeyCode::BackTab | KeyCode::Up => {
                self.save_current_edit_row();
                if self.edit_row > 0 {
                    self.edit_row -= 1;
                } else {
                    self.edit_row = total_rows - 1;
                }
                self.load_current_edit_row();
            }
            KeyCode::Enter => {
                self.save_current_edit_row();
                if let Some(session_id) = self.editing_session_id {
                    // Save name
                    if !self.edit_session_name.is_empty() {
                        self.db.update_session_name(session_id, &self.edit_session_name)?;
                    }
                    // Save all field values
                    for (i, field) in self.fields.iter().enumerate() {
                        if let Some(value) = self.edit_field_values.get(i) {
                            self.db.set_session_field_value(session_id, field.id, value)?;
                        }
                    }
                    self.refresh_sessions()?;
                }
                self.input_mode = InputMode::Normal;
                self.input_buffer.clear();
                self.editing_session_id = None;
                self.edit_session_name.clear();
                self.edit_field_values.clear();
                self.edit_mode = EditMode::Manual;
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

    fn run_ai_fill(&mut self) {
        use crate::ai;

        let fields: Vec<(String, String)> = self.fields
            .iter()
            .map(|f| (f.name.clone(), f.description.clone()))
            .collect();

        if fields.is_empty() {
            return;
        }

        // Get tmux pane content if available
        let pane_content = self.editing_session_id
            .and_then(|id| self.sessions.iter().find(|s| s.id == id))
            .and_then(|s| s.tmux_window.as_ref())
            .and_then(|name| tmux::capture_pane_content(name));

        match ai::fill_fields(&self.edit_session_name, &fields, pane_content.as_deref()) {
            Ok(values) => {
                // Update field values with AI suggestions
                for (i, value) in values.into_iter().enumerate() {
                    if i < self.edit_field_values.len() && !value.is_empty() {
                        self.edit_field_values[i] = value;
                    }
                }
                // Reload current row to reflect changes
                self.load_current_edit_row();
            }
            Err(_) => {
                // Silently fail - user can still edit manually
            }
        }
    }

    fn save_current_edit_row(&mut self) {
        if self.edit_row == 0 {
            self.edit_session_name = self.input_buffer.clone();
        } else {
            let field_idx = self.edit_row - 1;
            if field_idx < self.edit_field_values.len() {
                self.edit_field_values[field_idx] = self.input_buffer.clone();
            }
        }
    }

    fn load_current_edit_row(&mut self) {
        if self.edit_row == 0 {
            self.input_buffer = self.edit_session_name.clone();
        } else {
            let field_idx = self.edit_row - 1;
            if let Some(value) = self.edit_field_values.get(field_idx) {
                self.input_buffer = value.clone();
            } else {
                self.input_buffer.clear();
            }
        }
    }

    fn handle_move_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
                self.moving_session_id = None;
            }
            KeyCode::Char(c @ '1'..='4') => {
                let idx = (c as usize) - ('1' as usize);
                let statuses = Status::all();
                if idx < statuses.len() {
                    if let Some(session_id) = self.moving_session_id {
                        self.db.update_session_status(session_id, statuses[idx])?;
                        self.refresh_sessions()?;
                    }
                }
                self.input_mode = InputMode::Normal;
                self.moving_session_id = None;
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_settings_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.view = View::Kanban;
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if !self.fields.is_empty() && self.selected_field < self.fields.len() - 1 {
                    self.selected_field += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.selected_field > 0 {
                    self.selected_field -= 1;
                }
            }
            KeyCode::Char('n') => {
                self.new_field_name.clear();
                self.new_field_desc.clear();
                self.input_mode = InputMode::NewFieldName;
            }
            KeyCode::Char('e') => {
                if let Some(field) = self.fields.get(self.selected_field) {
                    self.editing_field_id = Some(field.id);
                    self.new_field_name = field.name.clone();
                    self.new_field_desc = field.description.clone();
                    self.input_mode = InputMode::EditFieldName;
                }
            }
            KeyCode::Char('d') => {
                if let Some(field) = self.fields.get(self.selected_field) {
                    self.db.delete_field(field.id)?;
                    self.refresh_fields()?;
                    if self.selected_field >= self.fields.len() && self.selected_field > 0 {
                        self.selected_field -= 1;
                    }
                }
            }
            KeyCode::Char('K') => {
                if let Some(field) = self.fields.get(self.selected_field) {
                    self.db.move_field_up(self.project.id, field.id)?;
                    self.refresh_fields()?;
                    if self.selected_field > 0 {
                        self.selected_field -= 1;
                    }
                }
            }
            KeyCode::Char('J') => {
                if let Some(field) = self.fields.get(self.selected_field) {
                    self.db.move_field_down(self.project.id, field.id)?;
                    self.refresh_fields()?;
                    if self.selected_field < self.fields.len() - 1 {
                        self.selected_field += 1;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_new_field_name_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
                self.new_field_name.clear();
                self.new_field_desc.clear();
            }
            KeyCode::Enter => {
                if !self.new_field_name.is_empty() {
                    self.input_mode = InputMode::NewFieldDesc;
                }
            }
            KeyCode::Backspace => {
                self.new_field_name.pop();
            }
            KeyCode::Char(c) => {
                self.new_field_name.push(c);
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_new_field_desc_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
                self.new_field_name.clear();
                self.new_field_desc.clear();
            }
            KeyCode::Enter => {
                self.db.create_field(self.project.id, &self.new_field_name, &self.new_field_desc)?;
                self.refresh_fields()?;
                self.input_mode = InputMode::Normal;
                self.new_field_name.clear();
                self.new_field_desc.clear();
            }
            KeyCode::Backspace => {
                self.new_field_desc.pop();
            }
            KeyCode::Char(c) => {
                self.new_field_desc.push(c);
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_edit_field_name_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
                self.editing_field_id = None;
                self.new_field_name.clear();
                self.new_field_desc.clear();
            }
            KeyCode::Enter => {
                if !self.new_field_name.is_empty() {
                    self.input_mode = InputMode::EditFieldDesc;
                }
            }
            KeyCode::Backspace => {
                self.new_field_name.pop();
            }
            KeyCode::Char(c) => {
                self.new_field_name.push(c);
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_edit_field_desc_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
                self.editing_field_id = None;
                self.new_field_name.clear();
                self.new_field_desc.clear();
            }
            KeyCode::Enter => {
                if let Some(field_id) = self.editing_field_id {
                    self.db.update_field(field_id, &self.new_field_name, &self.new_field_desc)?;
                    self.refresh_fields()?;
                }
                self.input_mode = InputMode::Normal;
                self.editing_field_id = None;
                self.new_field_name.clear();
                self.new_field_desc.clear();
            }
            KeyCode::Backspace => {
                self.new_field_desc.pop();
            }
            KeyCode::Char(c) => {
                self.new_field_desc.push(c);
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
