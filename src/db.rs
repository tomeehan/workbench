use color_eyre::Result;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    Planned,
    InProgress,
    Review,
    Done,
}

impl Status {
    pub fn as_str(&self) -> &'static str {
        match self {
            Status::Planned => "planned",
            Status::InProgress => "in_progress",
            Status::Review => "review",
            Status::Done => "done",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "planned" => Some(Status::Planned),
            "in_progress" => Some(Status::InProgress),
            "review" => Some(Status::Review),
            "done" => Some(Status::Done),
            _ => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Status::Planned => "Planned",
            Status::InProgress => "In Progress",
            Status::Review => "Review",
            Status::Done => "Done",
        }
    }

    pub fn all() -> &'static [Status] {
        &[Status::Planned, Status::InProgress, Status::Review, Status::Done]
    }
}

#[derive(Debug, Clone)]
pub struct Project {
    pub id: i64,
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone)]
pub struct Session {
    pub id: i64,
    pub project_id: i64,
    pub name: String,
    pub status: Status,
    pub checkout_path: Option<String>,
    pub branch_name: Option<String>,
    pub ticket_id: Option<String>,
    pub ticket_url: Option<String>,
    pub tmux_window: Option<String>,
    pub claude_session_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Field {
    pub id: i64,
    pub project_id: i64,
    pub name: String,
    pub description: String,
    pub display_order: i64,
}

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn new() -> Result<Self> {
        let db_path = Self::db_path()?;
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(&db_path)?;
        let db = Self { conn };
        db.init_schema()?;
        Ok(db)
    }

    fn db_path() -> Result<PathBuf> {
        let data_dir = dirs::data_dir()
            .ok_or_else(|| color_eyre::eyre::eyre!("Could not find data directory"))?;
        Ok(data_dir.join("workbench").join("workbench.db"))
    }

    fn init_schema(&self) -> Result<()> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS projects (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                path TEXT NOT NULL UNIQUE
            );

            CREATE TABLE IF NOT EXISTS sessions (
                id INTEGER PRIMARY KEY,
                project_id INTEGER NOT NULL,
                name TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'planned',
                checkout_path TEXT,
                branch_name TEXT,
                ticket_id TEXT,
                ticket_url TEXT,
                tmux_window TEXT,
                claude_session_id TEXT,
                created_at TEXT DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (project_id) REFERENCES projects(id)
            );

            CREATE TABLE IF NOT EXISTS fields (
                id INTEGER PRIMARY KEY,
                project_id INTEGER NOT NULL,
                name TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                display_order INTEGER NOT NULL DEFAULT 0,
                FOREIGN KEY (project_id) REFERENCES projects(id)
            );

            CREATE TABLE IF NOT EXISTS session_field_values (
                id INTEGER PRIMARY KEY,
                session_id INTEGER NOT NULL,
                field_id INTEGER NOT NULL,
                value TEXT NOT NULL DEFAULT '',
                FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE,
                FOREIGN KEY (field_id) REFERENCES fields(id) ON DELETE CASCADE,
                UNIQUE(session_id, field_id)
            );
            ",
        )?;
        Ok(())
    }

    pub fn get_or_create_project(&self, name: &str, path: &str) -> Result<Project> {
        if let Some(project) = self.get_project_by_path(path)? {
            return Ok(project);
        }

        self.conn.execute(
            "INSERT INTO projects (name, path) VALUES (?1, ?2)",
            params![name, path],
        )?;

        let id = self.conn.last_insert_rowid();
        Ok(Project {
            id,
            name: name.to_string(),
            path: path.to_string(),
        })
    }

    fn get_project_by_path(&self, path: &str) -> Result<Option<Project>> {
        let mut stmt = self.conn.prepare("SELECT id, name, path FROM projects WHERE path = ?1")?;
        let mut rows = stmt.query(params![path])?;

        if let Some(row) = rows.next()? {
            Ok(Some(Project {
                id: row.get(0)?,
                name: row.get(1)?,
                path: row.get(2)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn list_sessions(&self, project_id: i64) -> Result<Vec<Session>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, project_id, name, status, checkout_path, branch_name,
                    ticket_id, ticket_url, tmux_window, claude_session_id
             FROM sessions WHERE project_id = ?1 ORDER BY id",
        )?;

        let sessions = stmt.query_map(params![project_id], |row| {
            let status_str: String = row.get(3)?;
            Ok(Session {
                id: row.get(0)?,
                project_id: row.get(1)?,
                name: row.get(2)?,
                status: Status::from_str(&status_str).unwrap_or(Status::Planned),
                checkout_path: row.get(4)?,
                branch_name: row.get(5)?,
                ticket_id: row.get(6)?,
                ticket_url: row.get(7)?,
                tmux_window: row.get(8)?,
                claude_session_id: row.get(9)?,
            })
        })?;

        sessions.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn create_session(&self, project_id: i64, name: &str) -> Result<Session> {
        self.conn.execute(
            "INSERT INTO sessions (project_id, name, status) VALUES (?1, ?2, 'planned')",
            params![project_id, name],
        )?;

        let id = self.conn.last_insert_rowid();
        Ok(Session {
            id,
            project_id,
            name: name.to_string(),
            status: Status::Planned,
            checkout_path: None,
            branch_name: None,
            ticket_id: None,
            ticket_url: None,
            tmux_window: None,
            claude_session_id: None,
        })
    }

    pub fn update_session_status(&self, session_id: i64, status: Status) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET status = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
            params![status.as_str(), session_id],
        )?;
        Ok(())
    }

    pub fn update_session_name(&self, session_id: i64, name: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET name = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
            params![name, session_id],
        )?;
        Ok(())
    }

    pub fn delete_session(&self, session_id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM sessions WHERE id = ?1", params![session_id])?;
        Ok(())
    }

    pub fn set_tmux_session(&self, session_id: i64, tmux_name: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET tmux_window = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
            params![tmux_name, session_id],
        )?;
        Ok(())
    }

    pub fn clear_tmux_session(&self, session_id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET tmux_window = NULL, updated_at = CURRENT_TIMESTAMP WHERE id = ?1",
            params![session_id],
        )?;
        Ok(())
    }

    pub fn list_fields(&self, project_id: i64) -> Result<Vec<Field>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, project_id, name, description, display_order
             FROM fields WHERE project_id = ?1 ORDER BY display_order, id",
        )?;

        let fields = stmt.query_map(params![project_id], |row| {
            Ok(Field {
                id: row.get(0)?,
                project_id: row.get(1)?,
                name: row.get(2)?,
                description: row.get(3)?,
                display_order: row.get(4)?,
            })
        })?;

        fields.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn create_field(&self, project_id: i64, name: &str, description: &str) -> Result<Field> {
        let max_order: i64 = self.conn.query_row(
            "SELECT COALESCE(MAX(display_order), -1) FROM fields WHERE project_id = ?1",
            params![project_id],
            |row| row.get(0),
        )?;

        self.conn.execute(
            "INSERT INTO fields (project_id, name, description, display_order) VALUES (?1, ?2, ?3, ?4)",
            params![project_id, name, description, max_order + 1],
        )?;

        let id = self.conn.last_insert_rowid();
        Ok(Field {
            id,
            project_id,
            name: name.to_string(),
            description: description.to_string(),
            display_order: max_order + 1,
        })
    }

    pub fn update_field(&self, field_id: i64, name: &str, description: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE fields SET name = ?1, description = ?2 WHERE id = ?3",
            params![name, description, field_id],
        )?;
        Ok(())
    }

    pub fn delete_field(&self, field_id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM fields WHERE id = ?1", params![field_id])?;
        Ok(())
    }

    pub fn move_field_up(&self, project_id: i64, field_id: i64) -> Result<()> {
        let fields = self.list_fields(project_id)?;
        let idx = fields.iter().position(|f| f.id == field_id);
        if let Some(i) = idx {
            if i > 0 {
                let prev_id = fields[i - 1].id;
                let prev_order = fields[i - 1].display_order;
                let curr_order = fields[i].display_order;
                self.conn.execute("UPDATE fields SET display_order = ?1 WHERE id = ?2", params![prev_order, field_id])?;
                self.conn.execute("UPDATE fields SET display_order = ?1 WHERE id = ?2", params![curr_order, prev_id])?;
            }
        }
        Ok(())
    }

    pub fn move_field_down(&self, project_id: i64, field_id: i64) -> Result<()> {
        let fields = self.list_fields(project_id)?;
        let idx = fields.iter().position(|f| f.id == field_id);
        if let Some(i) = idx {
            if i < fields.len() - 1 {
                let next_id = fields[i + 1].id;
                let next_order = fields[i + 1].display_order;
                let curr_order = fields[i].display_order;
                self.conn.execute("UPDATE fields SET display_order = ?1 WHERE id = ?2", params![next_order, field_id])?;
                self.conn.execute("UPDATE fields SET display_order = ?1 WHERE id = ?2", params![curr_order, next_id])?;
            }
        }
        Ok(())
    }

    pub fn get_session_field_value(&self, session_id: i64, field_id: i64) -> Result<String> {
        let result: Result<String, _> = self.conn.query_row(
            "SELECT value FROM session_field_values WHERE session_id = ?1 AND field_id = ?2",
            params![session_id, field_id],
            |row| row.get(0),
        );
        Ok(result.unwrap_or_default())
    }

    pub fn set_session_field_value(&self, session_id: i64, field_id: i64, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO session_field_values (session_id, field_id, value) VALUES (?1, ?2, ?3)
             ON CONFLICT(session_id, field_id) DO UPDATE SET value = ?3",
            params![session_id, field_id, value],
        )?;
        Ok(())
    }

    pub fn get_all_session_field_values(&self, session_id: i64) -> Result<Vec<(i64, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT field_id, value FROM session_field_values WHERE session_id = ?1",
        )?;
        let values = stmt.query_map(params![session_id], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?;
        values.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}
