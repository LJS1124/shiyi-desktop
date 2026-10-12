use crate::time::Clock;
use crate::{Error, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY, value TEXT);
CREATE TABLE IF NOT EXISTS entities(
  id TEXT PRIMARY KEY,
  type TEXT NOT NULL,
  doc TEXT NOT NULL,
  title TEXT,
  status TEXT,
  project_id TEXT,
  created_at TEXT,
  updated_at TEXT,
  deleted_at TEXT,
  version INTEGER NOT NULL DEFAULT 1,
  created_by TEXT
);
CREATE INDEX IF NOT EXISTS entities_type ON entities(type, deleted_at);
CREATE INDEX IF NOT EXISTS entities_project ON entities(project_id);
CREATE TABLE IF NOT EXISTS ops(
  seq INTEGER PRIMARY KEY AUTOINCREMENT,
  id TEXT NOT NULL UNIQUE,
  batch TEXT NOT NULL,
  ts TEXT NOT NULL,
  ts_ms INTEGER NOT NULL,
  actor TEXT NOT NULL,
  op TEXT NOT NULL,
  entity_type TEXT,
  entity_id TEXT,
  before TEXT,
  after TEXT,
  summary TEXT,
  note TEXT,
  undone_by TEXT
);
CREATE INDEX IF NOT EXISTS ops_batch ON ops(batch);
CREATE INDEX IF NOT EXISTS ops_entity ON ops(entity_id);
CREATE INDEX IF NOT EXISTS ops_actor ON ops(actor, seq);
"#;

pub const SCHEMA_VERSION: &str = "1";

/// Where the app keeps its data. `SHIYI_HOME` overrides.
pub fn data_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("SHIYI_HOME") {
        return PathBuf::from(d);
    }
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    #[cfg(target_os = "macos")]
    {
        home.join("Library").join("Application Support").join("app.shiyi.desktop")
    }
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or(home).join("app.shiyi.desktop")
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| home.join(".local").join("share"))
            .join("app.shiyi.desktop")
    }
}

/// `SHIYI_DB` overrides; otherwise `<data dir>/shiyi.db`.
pub fn default_db_path() -> PathBuf {
    match std::env::var_os("SHIYI_DB") {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => data_dir().join("shiyi.db"),
    }
}

pub struct Core {
    pub(crate) conn: Connection,
    path: Option<PathBuf>,
    fixed_clock: Option<Clock>,
}

impl Core {
    pub fn open(path: impl AsRef<Path>) -> Result<Core> {
        let path = path.as_ref();
        if let Some(dir) = path.parent() {
            if !dir.as_os_str().is_empty() {
                std::fs::create_dir_all(dir)?;
            }
        }
        let conn = Connection::open(path)?;
        let core = Core { conn, path: Some(path.to_path_buf()), fixed_clock: None };
        core.init(true)?;
        Ok(core)
    }

    pub fn open_default() -> Result<Core> {
        Core::open(default_db_path())
    }

    pub fn open_in_memory() -> Result<Core> {
        let core = Core { conn: Connection::open_in_memory()?, path: None, fixed_clock: None };
        core.init(false)?;
        Ok(core)
    }

    fn init(&self, wal: bool) -> Result<()> {
        self.conn.busy_timeout(std::time::Duration::from_millis(8000))?;
        if wal {
            let _: String = self.conn.query_row("PRAGMA journal_mode=WAL", [], |r| r.get(0))?;
            self.conn.execute_batch("PRAGMA synchronous=NORMAL;")?;
        }
        self.conn.execute_batch(SCHEMA)?;
        self.conn.execute("INSERT OR IGNORE INTO meta(key,value) VALUES('schema_version',?1)", [SCHEMA_VERSION])?;
        self.conn.execute("INSERT OR IGNORE INTO meta(key,value) VALUES('agent_permission','write')", [])?;
        Ok(())
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Pins "now" (tests, reproducible demos).
    pub fn set_fixed_clock(&mut self, now_ms: i64, offset_s: i64) {
        self.fixed_clock = Some(Clock { now_ms, offset_s });
    }

    pub fn clock(&self) -> Clock {
        if let Some(c) = &self.fixed_clock {
            return c.clone();
        }
        let off: i64 = self
            .conn
            .query_row("SELECT CAST(strftime('%s','now','localtime') AS INTEGER) - CAST(strftime('%s','now') AS INTEGER)", [], |r| r.get(0))
            .unwrap_or(0);
        Clock::new(off)
    }

    pub fn meta_get(&self, key: &str) -> Result<Option<String>> {
        Ok(self.conn.query_row("SELECT value FROM meta WHERE key=?1", [key], |r| r.get(0)).optional()?)
    }

    pub(crate) fn meta_set(conn: &Connection, key: &str, value: &str) -> Result<()> {
        conn.execute("INSERT INTO meta(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![key, value])?;
        Ok(())
    }

    /// App bookkeeping (first run, onboarding). Use the `settings.set` op for user settings.
    pub fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        if key == "agent_permission" || key == "schema_version" {
            return Err(Error::new("invalid_value", "use the settings.set op for this key"));
        }
        Core::meta_set(&self.conn, key, value)
    }

    /// What agents may do: `read`, `write` (default) or `full`.
    pub fn permission(&self) -> String {
        self.meta_get("agent_permission").ok().flatten().unwrap_or_else(|| "write".into())
    }

    /// Changes whenever another connection (another process) commits.
    pub fn data_version(&self) -> i64 {
        self.conn.query_row("PRAGMA data_version", [], |r| r.get(0)).unwrap_or(0)
    }

    pub fn max_seq(&self) -> i64 {
        self.conn.query_row("SELECT COALESCE(MAX(seq),0) FROM ops", [], |r| r.get(0)).unwrap_or(0)
    }

    pub fn is_empty(&self) -> Result<bool> {
        let n: i64 = self.conn.query_row("SELECT COUNT(*) FROM entities", [], |r| r.get(0))?;
        Ok(n == 0)
    }

    /// Applies a request of operations as one atomic, undoable batch.
    pub fn apply(&mut self, req: Value, actor: &str) -> Result<Value> {
        crate::ops::apply(self, req, actor)
    }

    /// Runs a read-only query.
    pub fn query(&self, req: Value) -> Result<Value> {
        crate::query::query(self, req)
    }

    /// Imports the old `data.json` format (or a snapshot export) as one undoable batch.
    pub fn import(&mut self, data: Value, actor: &str) -> Result<Value> {
        crate::legacy::import(self, data, actor)
    }

    /// First launch after the upgrade: moves the old `data.json` into the database once.
    pub fn import_legacy_file_once(&mut self, file: &Path) -> Result<Option<Value>> {
        if self.meta_get("legacy_imported")?.is_some() || !file.exists() {
            return Ok(None);
        }
        let text = std::fs::read_to_string(file)?;
        let data: Value = serde_json::from_str(&text)?;
        let res = self.import(data, "user")?;
        Core::meta_set(&self.conn, "legacy_imported", &self.clock().now_iso())?;
        let done = file.with_extension("json.imported");
        let _ = std::fs::rename(file, done);
        Ok(Some(res))
    }

    /// One backup per day in `<db dir>/backups`, keeping the newest `keep`.
    /// Also trims the operation log to the last 180 days.
    pub fn backup_daily(&self, keep: usize) -> Result<Option<PathBuf>> {
        let Some(path) = &self.path else { return Ok(None) };
        let dir = path.parent().map(|p| p.join("backups")).unwrap_or_else(|| PathBuf::from("backups"));
        std::fs::create_dir_all(&dir)?;
        let clock = self.clock();
        let target = dir.join(format!("shiyi-{}.db", clock.today()));
        let mut made = None;
        if !target.exists() {
            self.conn.execute("VACUUM INTO ?1", [target.to_string_lossy().as_ref()])?;
            made = Some(target);
        }
        let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.file_name().and_then(|n| n.to_str()).map_or(false, |n| n.starts_with("shiyi-") && n.ends_with(".db")))
            .collect();
        files.sort();
        while files.len() > keep {
            let old = files.remove(0);
            let _ = std::fs::remove_file(old);
        }
        let cutoff = clock.now_ms - 180 * 86_400_000;
        self.conn.execute("DELETE FROM ops WHERE ts_ms < ?1", [cutoff])?;
        Ok(made)
    }
}

pub(crate) fn parse_doc(text: &str) -> Result<Map<String, Value>> {
    match serde_json::from_str::<Value>(text)? {
        Value::Object(o) => Ok(o),
        _ => Err(Error::new("db_error", "stored document is not an object")),
    }
}

pub(crate) fn load_doc(conn: &Connection, id: &str) -> Result<Option<Map<String, Value>>> {
    let text: Option<String> = conn.query_row("SELECT doc FROM entities WHERE id=?1", [id], |r| r.get(0)).optional()?;
    text.map(|t| parse_doc(&t)).transpose()
}

/// Documents of one type (or all types), oldest first.
pub(crate) fn docs(conn: &Connection, ty: Option<&str>, deleted: Deleted) -> Result<Vec<Map<String, Value>>> {
    let cond = match deleted {
        Deleted::Exclude => "deleted_at IS NULL",
        Deleted::Include => "1=1",
        Deleted::Only => "deleted_at IS NOT NULL",
    };
    let sql = match ty {
        Some(_) => format!("SELECT doc FROM entities WHERE type=?1 AND {} ORDER BY created_at, id", cond),
        None => format!("SELECT doc FROM entities WHERE {} ORDER BY created_at, id", cond),
    };
    let mut st = conn.prepare(&sql)?;
    let rows: Vec<String> = match ty {
        Some(t) => st.query_map([t], |r| r.get(0))?.collect::<std::result::Result<_, _>>()?,
        None => st.query_map([], |r| r.get(0))?.collect::<std::result::Result<_, _>>()?,
    };
    rows.iter().map(|t| parse_doc(t)).collect()
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Deleted {
    Exclude,
    Include,
    Only,
}

pub(crate) fn write_doc(conn: &Connection, doc: &Map<String, Value>) -> Result<()> {
    let g = |k: &str| doc.get(k).and_then(Value::as_str).map(String::from);
    let title = crate::model::title_of(doc);
    let version = doc.get("version").and_then(Value::as_i64).unwrap_or(1);
    conn.execute(
        "INSERT OR REPLACE INTO entities(id,type,doc,title,status,project_id,created_at,updated_at,deleted_at,version,created_by)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
        params![
            g("id"),
            g("type"),
            serde_json::to_string(doc)?,
            title,
            g("status"),
            g("project_id"),
            g("created_at"),
            g("updated_at"),
            g("deleted_at"),
            version,
            g("created_by")
        ],
    )?;
    Ok(())
}

pub(crate) fn delete_row(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM entities WHERE id=?1", [id])?;
    Ok(())
}
