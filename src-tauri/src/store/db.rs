use crate::audio::Track;
use crate::error::{AppError, Result};
use crate::paths;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::sync::Mutex;

const SCHEMA_VERSION: i64 = 1;

#[derive(Debug, Clone, Serialize)]
pub struct SessionMeta {
    pub id: String,
    pub title: String,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub duration_ms: Option<i64>,
    pub model_used: Option<String>,
    pub source_name: Option<String>,
    pub lang_mode: String,
    pub system_wav_path: Option<String>,
    pub mic_wav_path: Option<String>,
    pub segment_count: i64,
    pub word_count: i64,
    pub has_summary: bool,
    /// Only populated on search results.
    pub snippet: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Segment {
    pub id: i64,
    pub session_id: String,
    pub track: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
    pub lang: Option<String>,
    pub confidence: Option<f32>,
    pub edited: bool,
}

pub struct NewSession<'a> {
    pub id: &'a str,
    pub title: &'a str,
    pub started_at: i64,
    pub model_used: &'a str,
    pub source_name: &'a str,
    pub lang_mode: &'a str,
    pub system_wav: Option<&'a str>,
    pub mic_wav: Option<&'a str>,
}

pub struct NewSegment<'a> {
    pub session_id: &'a str,
    pub track: Track,
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: &'a str,
    pub lang: Option<&'a str>,
    pub confidence: Option<f32>,
}

pub struct Db {
    conn: Mutex<Connection>,
}

impl Db {
    pub fn open() -> Result<Self> {
        let path = paths::db_path()?;
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.migrate()?;
        Ok(db)
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
        self.conn
            .lock()
            .map_err(|_| AppError::Db("the database connection is permanently poisoned".into()))
    }

    fn migrate(&self) -> Result<()> {
        let conn = self.lock()?;
        let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if version >= SCHEMA_VERSION {
            return Ok(());
        }
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS sessions (
              id              TEXT PRIMARY KEY,
              title           TEXT NOT NULL,
              started_at      INTEGER NOT NULL,
              ended_at        INTEGER,
              duration_ms     INTEGER,
              model_used      TEXT,
              source_name     TEXT,
              lang_mode       TEXT NOT NULL DEFAULT 'auto',
              system_wav_path TEXT,
              mic_wav_path    TEXT
            );

            CREATE TABLE IF NOT EXISTS segments (
              id          INTEGER PRIMARY KEY AUTOINCREMENT,
              session_id  TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
              track       TEXT NOT NULL,
              start_ms    INTEGER NOT NULL,
              end_ms      INTEGER NOT NULL,
              text        TEXT NOT NULL,
              lang        TEXT,
              confidence  REAL,
              edited      INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_seg_session ON segments(session_id, start_ms);

            CREATE VIRTUAL TABLE IF NOT EXISTS segments_fts USING fts5(
              text, content='segments', content_rowid='id', tokenize='unicode61'
            );
            CREATE TRIGGER IF NOT EXISTS segments_ai AFTER INSERT ON segments BEGIN
              INSERT INTO segments_fts(rowid, text) VALUES (new.id, new.text);
            END;
            CREATE TRIGGER IF NOT EXISTS segments_ad AFTER DELETE ON segments BEGIN
              INSERT INTO segments_fts(segments_fts, rowid, text) VALUES('delete', old.id, old.text);
            END;
            CREATE TRIGGER IF NOT EXISTS segments_au AFTER UPDATE ON segments BEGIN
              INSERT INTO segments_fts(segments_fts, rowid, text) VALUES('delete', old.id, old.text);
              INSERT INTO segments_fts(rowid, text) VALUES (new.id, new.text);
            END;

            CREATE TABLE IF NOT EXISTS summaries (
              session_id  TEXT PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE,
              provider    TEXT NOT NULL,
              model       TEXT NOT NULL,
              content     TEXT NOT NULL,
              created_at  INTEGER NOT NULL
            );
            "#,
        )?;
        conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        tracing::info!("database schema migrated to version {SCHEMA_VERSION}");
        Ok(())
    }

    pub fn create_session(&self, s: &NewSession) -> Result<()> {
        self.lock()?.execute(
            "INSERT INTO sessions (id,title,started_at,model_used,source_name,lang_mode,system_wav_path,mic_wav_path)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                s.id,
                s.title,
                s.started_at,
                s.model_used,
                s.source_name,
                s.lang_mode,
                s.system_wav,
                s.mic_wav
            ],
        )?;
        Ok(())
    }

    pub fn finish_session(&self, id: &str, ended_at: i64, duration_ms: i64) -> Result<()> {
        self.lock()?.execute(
            "UPDATE sessions SET ended_at=?2, duration_ms=?3 WHERE id=?1",
            params![id, ended_at, duration_ms],
        )?;
        Ok(())
    }

    /// Segments are written one at a time rather than batched: at the rate people speak this
    /// is a few inserts per minute, and it means each segment is durable the moment it
    /// exists.
    pub fn insert_segment(&self, s: &NewSegment) -> Result<i64> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO segments (session_id,track,start_ms,end_ms,text,lang,confidence)
             VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![
                s.session_id,
                s.track.as_str(),
                s.start_ms,
                s.end_ms,
                s.text,
                s.lang,
                s.confidence
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn edit_segment(&self, id: i64, text: &str) -> Result<()> {
        let n = self.lock()?.execute(
            "UPDATE segments SET text=?2, edited=1 WHERE id=?1",
            params![id, text],
        )?;
        if n == 0 {
            return Err(AppError::NotFound(format!("segment {id}")));
        }
        Ok(())
    }

    pub fn rename_session(&self, id: &str, title: &str) -> Result<()> {
        let n = self
            .lock()?
            .execute("UPDATE sessions SET title=?2 WHERE id=?1", params![id, title])?;
        if n == 0 {
            return Err(AppError::NotFound(format!("session {id}")));
        }
        Ok(())
    }

    pub fn list_sessions(&self, q: Option<&str>, limit: i64, offset: i64) -> Result<Vec<SessionMeta>> {
        const COLS: &str = r#"
            s.id, s.title, s.started_at, s.ended_at, s.duration_ms, s.model_used,
            s.source_name, s.lang_mode, s.system_wav_path, s.mic_wav_path,
            (SELECT COUNT(*) FROM segments g WHERE g.session_id=s.id),
            (SELECT COALESCE(SUM(LENGTH(g.text) - LENGTH(REPLACE(g.text,' ','')) + 1),0)
               FROM segments g WHERE g.session_id=s.id),
            (SELECT COUNT(*) FROM summaries m WHERE m.session_id=s.id)
        "#;

        let conn = self.lock()?;
        let mut out = Vec::new();

        match q.map(str::trim).filter(|s| !s.is_empty()) {
            None => {
                let sql = format!(
                    "SELECT {COLS} FROM sessions s ORDER BY s.started_at DESC LIMIT ?1 OFFSET ?2"
                );
                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt.query_map(params![limit, offset], |r| Ok(row_to_meta(r, None)))?;
                for r in rows {
                    out.push(r?);
                }
            }
            Some(query) => {
                // Cross-session search through FTS5. One row per session, with a snippet
                // from the best-matching segment.
                let sql = format!(
                    r#"
                    WITH hits AS (
                      SELECT g.session_id AS sid,
                             snippet(segments_fts, 0, '«', '»', '…', 12) AS snip,
                             MIN(rank) AS best
                      FROM segments_fts
                      JOIN segments g ON g.id = segments_fts.rowid
                      WHERE segments_fts MATCH ?1
                      GROUP BY g.session_id
                    )
                    SELECT {COLS}, hits.snip
                    FROM sessions s
                    JOIN hits ON hits.sid = s.id
                    ORDER BY hits.best ASC, s.started_at DESC
                    LIMIT ?2 OFFSET ?3
                    "#
                );
                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt.query_map(params![to_fts_query(query), limit, offset], |r| {
                    let snip: Option<String> = r.get(13).ok();
                    Ok(row_to_meta(r, snip))
                })?;
                for r in rows {
                    out.push(r?);
                }
            }
        }
        Ok(out)
    }

    pub fn get_session(&self, id: &str) -> Result<SessionMeta> {
        let conn = self.lock()?;
        let sql = r#"
            SELECT s.id, s.title, s.started_at, s.ended_at, s.duration_ms, s.model_used,
                   s.source_name, s.lang_mode, s.system_wav_path, s.mic_wav_path,
                   (SELECT COUNT(*) FROM segments g WHERE g.session_id=s.id),
                   (SELECT COALESCE(SUM(LENGTH(g.text) - LENGTH(REPLACE(g.text,' ','')) + 1),0)
                      FROM segments g WHERE g.session_id=s.id),
                   (SELECT COUNT(*) FROM summaries m WHERE m.session_id=s.id)
            FROM sessions s WHERE s.id=?1
        "#;
        conn.query_row(sql, params![id], |r| Ok(row_to_meta(r, None)))
            .optional()?
            .ok_or_else(|| AppError::NotFound(format!("session {id}")))
    }

    /// An empty `tracks` means every track. The transcript view only asks for `system`.
    pub fn get_segments(&self, session_id: &str, tracks: &[Track]) -> Result<Vec<Segment>> {
        let conn = self.lock()?;
        let (sql, filter): (String, Vec<String>) = if tracks.is_empty() {
            (
                "SELECT id,session_id,track,start_ms,end_ms,text,lang,confidence,edited
                 FROM segments WHERE session_id=?1 ORDER BY start_ms ASC, id ASC"
                    .into(),
                vec![],
            )
        } else {
            let placeholders = (0..tracks.len())
                .map(|i| format!("?{}", i + 2))
                .collect::<Vec<_>>()
                .join(",");
            (
                format!(
                    "SELECT id,session_id,track,start_ms,end_ms,text,lang,confidence,edited
                     FROM segments WHERE session_id=?1 AND track IN ({placeholders})
                     ORDER BY start_ms ASC, id ASC"
                ),
                tracks.iter().map(|t| t.as_str().to_string()).collect(),
            )
        };
        let mut stmt = conn.prepare(&sql)?;
        let mut binds: Vec<&dyn rusqlite::ToSql> = vec![&session_id];
        for f in &filter {
            binds.push(f);
        }
        let rows = stmt.query_map(binds.as_slice(), |r| {
            Ok(Segment {
                id: r.get(0)?,
                session_id: r.get(1)?,
                track: r.get(2)?,
                start_ms: r.get(3)?,
                end_ms: r.get(4)?,
                text: r.get(5)?,
                lang: r.get(6)?,
                confidence: r.get(7)?,
                edited: r.get::<_, i64>(8)? != 0,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn delete_session(&self, id: &str) -> Result<()> {
        self.lock()?
            .execute("DELETE FROM sessions WHERE id=?1", params![id])?;
        Ok(())
    }

    pub fn unfinished_sessions(&self) -> Result<Vec<SessionMeta>> {
        let all = self.list_sessions(None, 200, 0)?;
        Ok(all.into_iter().filter(|s| s.ended_at.is_none()).collect())
    }

    pub fn save_summary(
        &self,
        session_id: &str,
        provider: &str,
        model: &str,
        content: &str,
        created_at: i64,
    ) -> Result<()> {
        self.lock()?.execute(
            "INSERT INTO summaries (session_id,provider,model,content,created_at)
             VALUES (?1,?2,?3,?4,?5)
             ON CONFLICT(session_id) DO UPDATE SET provider=?2, model=?3, content=?4, created_at=?5",
            params![session_id, provider, model, content, created_at],
        )?;
        Ok(())
    }

    pub fn get_summary(&self, session_id: &str) -> Result<Option<String>> {
        Ok(self
            .lock()?
            .query_row(
                "SELECT content FROM summaries WHERE session_id=?1",
                params![session_id],
                |r| r.get::<_, String>(0),
            )
            .optional()?)
    }
}

fn row_to_meta(r: &rusqlite::Row, snippet: Option<String>) -> SessionMeta {
    SessionMeta {
        id: r.get(0).unwrap_or_default(),
        title: r.get(1).unwrap_or_default(),
        started_at: r.get(2).unwrap_or(0),
        ended_at: r.get(3).ok().flatten(),
        duration_ms: r.get(4).ok().flatten(),
        model_used: r.get(5).ok().flatten(),
        source_name: r.get(6).ok().flatten(),
        lang_mode: r.get(7).unwrap_or_else(|_| "auto".into()),
        system_wav_path: r.get(8).ok().flatten(),
        mic_wav_path: r.get(9).ok().flatten(),
        segment_count: r.get(10).unwrap_or(0),
        word_count: r.get(11).unwrap_or(0),
        has_summary: r.get::<_, i64>(12).unwrap_or(0) > 0,
        snippet,
    }
}

/// Turns free-form search input into a safe FTS5 query: each word becomes a prefix term, and
/// quotes and operators are stripped so a user cannot trigger a syntax error.
fn to_fts_query(input: &str) -> String {
    let cleaned: Vec<String> = input
        .split_whitespace()
        .map(|w| {
            w.chars()
                .filter(|c| c.is_alphanumeric() || *c == '-')
                .collect::<String>()
        })
        .filter(|w| !w.is_empty())
        .map(|w| format!("\"{w}\"*"))
        .collect();
    if cleaned.is_empty() {
        "\"\"".into()
    } else {
        cleaned.join(" AND ")
    }
}

impl Db {
    /// Deletes every segment of one track in one session — used before re-transcribing so
    /// the results do not double up.
    pub fn delete_segments(&self, session_id: &str, track: Track) -> Result<usize> {
        Ok(self.lock()?.execute(
            "DELETE FROM segments WHERE session_id=?1 AND track=?2",
            params![session_id, track.as_str()],
        )?)
    }
}
