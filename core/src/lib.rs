//! 拾忆 core.
//!
//! One SQLite file holds every idea, project, task, mind map and link as a JSON
//! document. Every change goes through [`Core::apply`], which validates the
//! request, writes the documents and records an operation log entry with the
//! before/after snapshot, so any change (by the user or by an AI agent) can be
//! reviewed and undone. Reads go through [`Core::query`].
//!
//! The same crate builds the `shiyi` command line tool, which is how AI agents
//! use the app: JSON in, JSON out.

pub mod ai;
pub mod cli;
mod db;
mod graph;
pub mod install;
mod legacy;
pub mod model;
mod ops;
pub mod providers;
mod query;
mod related;
pub mod time;
pub mod update;
mod util;

pub use db::{data_dir, default_db_path, Core};

use serde_json::{json, Value};
use std::fmt;

/// The skill that teaches AI agents how to use the CLI.
pub const SKILL_MD: &str = include_str!("../../skills/shiyi/SKILL.md");
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub type Result<T> = std::result::Result<T, Error>;

/// Every failure has a stable machine-readable `code`, a message and,
/// where it helps, a hint that says how to fix the request.
#[derive(Debug, Clone)]
pub struct Error {
    pub code: String,
    pub message: String,
    pub hint: Option<String>,
    pub data: Option<Value>,
}

impl Error {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Error { code: code.to_string(), message: message.into(), hint: None, data: None }
    }
    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
    pub fn data(mut self, data: Value) -> Self {
        self.data = Some(data);
        self
    }
    pub fn to_json(&self) -> Value {
        let mut e = json!({ "code": self.code, "message": self.message });
        if let Some(h) = &self.hint {
            e["hint"] = json!(h);
        }
        if let Some(d) = &self.data {
            if let Some(o) = d.as_object() {
                for (k, v) in o {
                    e[k] = v.clone();
                }
            }
        }
        json!({ "ok": false, "error": e })
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)?;
        if let Some(h) = &self.hint {
            write!(f, " ({})", h)?;
        }
        Ok(())
    }
}

impl std::error::Error for Error {}

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        let busy = matches!(&e, rusqlite::Error::SqliteFailure(f, _) if f.code == rusqlite::ErrorCode::DatabaseBusy || f.code == rusqlite::ErrorCode::DatabaseLocked);
        if busy {
            Error::new("busy", "the database is busy").hint("another process is writing; retry in a moment")
        } else {
            Error::new("db_error", e.to_string())
        }
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::new("invalid_json", e.to_string()).hint("the request must be valid JSON")
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::new("io_error", e.to_string())
    }
}
