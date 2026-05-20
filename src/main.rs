//! recall — find any past Claude Code / Codex session by fuzzy memory.
//!
//! Single-node, local-first. SQLite + FTS5. No embeddings, no API keys, no network.
//! Calls no external commands; reports the `/resume <uuid>` slash-command line
//! for the user to paste into their current CLI session.

use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use clap::{Parser, Subcommand};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS sessions (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    provider        TEXT    NOT NULL,
    session_id      TEXT    NOT NULL,
    cwd             TEXT,
    source_path     TEXT    NOT NULL,
    title           TEXT,
    first_prompt    TEXT,
    last_prompt     TEXT,
    first_ts        TEXT,
    last_ts         TEXT,
    message_count   INTEGER NOT NULL DEFAULT 0,
    user_msg_count  INTEGER NOT NULL DEFAULT 0,
    asst_msg_count  INTEGER NOT NULL DEFAULT 0,
    file_size       INTEGER NOT NULL DEFAULT 0,
    content_sha256  TEXT,
    indexed_at      TEXT    NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at      TEXT    NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(provider, session_id)
);
CREATE INDEX IF NOT EXISTS idx_sessions_last_ts  ON sessions(last_ts DESC);
CREATE INDEX IF NOT EXISTS idx_sessions_cwd      ON sessions(cwd);
CREATE INDEX IF NOT EXISTS idx_sessions_provider ON sessions(provider);

CREATE VIRTUAL TABLE IF NOT EXISTS sessions_fts USING fts5(
    session_pk UNINDEXED,
    title,
    first_prompt,
    last_prompt,
    body,
    tokenize = 'unicode61 remove_diacritics 2'
);

CREATE TABLE IF NOT EXISTS edges (
    src_session_pk  INTEGER NOT NULL,
    dst_session_pk  INTEGER NOT NULL,
    kind            TEXT    NOT NULL,
    weight          REAL    NOT NULL DEFAULT 1.0,
    PRIMARY KEY (src_session_pk, dst_session_pk, kind),
    FOREIGN KEY (src_session_pk) REFERENCES sessions(id) ON DELETE CASCADE,
    FOREIGN KEY (dst_session_pk) REFERENCES sessions(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_edges_src ON edges(src_session_pk);
CREATE INDEX IF NOT EXISTS idx_edges_dst ON edges(dst_session_pk);
"#;

#[derive(Parser)]
#[command(name = "recall", version, about = "Find any past Claude/Codex session by fuzzy memory")]
struct Cli {
    /// SQLite DB 경로 (default: ~/.recall/recall.db)
    #[arg(long, global = true)]
    db: Option<PathBuf>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// DB 초기화 (idempotent)
    Init,
    /// 로컬 세션 파일 인덱싱 (Claude Code + Codex)
    Scan {
        #[arg(long, default_value = "all")]
        provider: String,
        #[arg(long)]
        force: bool,
    },
    /// FTS5 풀텍스트 검색
    Search {
        keyword: String,
        #[arg(long, short, default_value_t = 20)]
        n: usize,
        #[arg(long)]
        provider: Option<String>,
    },
    /// 세션 상세 (session_id 또는 prefix)
    Show {
        session_id_prefix: String,
    },
    /// 세션 찾기 — session_id 와 사용자가 현재 CLI 에 붙여넣을 `/resume <uuid>` 한 줄을 출력.
    /// 아무 프로세스도 spawn 하지 않음.
    Resume {
        query: String,
    },
    /// 같은 cwd 의 다른 세션 (1-hop 그래프)
    Related {
        session_id_prefix: String,
        #[arg(long, short, default_value_t = 10)]
        n: usize,
    },
    /// provider 별 세션/메시지/사이즈 통계
    Stats,
    /// 주기적 자동 인덱싱 — OS 스케줄러 관리
    Daemon {
        #[command(subcommand)]
        action: DaemonAction,
    },
}

#[derive(Subcommand)]
enum DaemonAction {
    /// OS 스케줄러에 주기 `recall scan` 등록 (Linux/macOS: crontab, Windows: Scheduled Task)
    Install {
        #[arg(long, default_value_t = 30)]
        interval_min: u32,
    },
    /// 등록된 자동 인덱싱 제거
    Uninstall,
    /// 등록 상태 확인
    Status,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let db_path = cli.db.unwrap_or_else(default_db_path);
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let mut conn = Connection::open(&db_path).context("open SQLite DB")?;
    conn.execute_batch(SCHEMA_SQL).context("apply schema")?;

    match cli.cmd {
        Cmd::Init => println!("[recall] DB ready at {}", db_path.display()),
        Cmd::Scan { provider, force } => cmd_scan(&mut conn, &provider, force)?,
        Cmd::Search { keyword, n, provider } => cmd_search(&conn, &keyword, n, provider.as_deref())?,
        Cmd::Show { session_id_prefix } => cmd_show(&conn, &session_id_prefix)?,
        Cmd::Resume { query } => cmd_resume(&conn, &query)?,
        Cmd::Related { session_id_prefix, n } => cmd_related(&conn, &session_id_prefix, n)?,
        Cmd::Stats => cmd_stats(&conn)?,
        Cmd::Daemon { action } => cmd_daemon(action)?,
    }
    Ok(())
}

fn default_db_path() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".recall").join("recall.db")
}

fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

fn claude_projects_root() -> PathBuf {
    home().join(".claude").join("projects")
}

fn codex_history_path() -> PathBuf {
    home().join(".codex").join("history.jsonl")
}

fn codex_sessions_root() -> PathBuf {
    home().join(".codex").join("sessions")
}

fn codex_session_index_path() -> PathBuf {
    home().join(".codex").join("session_index.jsonl")
}

fn load_codex_session_index() -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    let path = codex_session_index_path();
    let body = match std::fs::read_to_string(&path) { Ok(b) => b, Err(_) => return map };
    for line in body.lines() {
        if line.trim().is_empty() { continue; }
        let v: Value = match serde_json::from_str(line) { Ok(v) => v, Err(_) => continue };
        let id = v.get("id").and_then(|s| s.as_str()).unwrap_or("");
        let name = v.get("thread_name").and_then(|s| s.as_str()).unwrap_or("");
        if !id.is_empty() && !name.is_empty() {
            map.insert(id.to_string(), name.to_string());
        }
    }
    map
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct SessionRow {
    provider: String,
    session_id: String,
    cwd: Option<String>,
    source_path: String,
    title: Option<String>,
    first_prompt: Option<String>,
    last_prompt: Option<String>,
    first_ts: Option<NaiveDateTime>,
    last_ts: Option<NaiveDateTime>,
    message_count: i32,
    user_msg_count: i32,
    asst_msg_count: i32,
    file_size: i64,
    content_sha256: String,
    body_full: String,
}

fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    hex::encode(h.finalize())
}

fn truncate_chars(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

// ----- scan -----

fn cmd_scan(conn: &mut Connection, provider_filter: &str, force: bool) -> Result<()> {
    println!("[recall scan] provider={} force={}", provider_filter, force);

    let mut known: std::collections::HashMap<(String, String), String> = std::collections::HashMap::new();
    {
        let mut stmt = conn.prepare("SELECT provider, session_id, content_sha256 FROM sessions")?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?))
        })?;
        for row in rows {
            let (p, s, h) = row?;
            if let Some(h) = h { known.insert((p, s), h); }
        }
    }
    println!("[recall scan] known sessions in DB: {}", known.len());

    let mut scanned = 0u64;
    let mut upserted = 0u64;
    let mut skipped = 0u64;
    let mut errors = 0u64;

    if provider_filter == "all" || provider_filter == "claude" {
        let root = claude_projects_root();
        if root.is_dir() {
            for entry in walkdir::WalkDir::new(&root).into_iter().filter_map(|e| e.ok()) {
                if !entry.file_type().is_file() { continue; }
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("jsonl") { continue; }
                scanned += 1;
                match parse_claude_jsonl(path) {
                    Ok(rec) => {
                        let key = (rec.provider.clone(), rec.session_id.clone());
                        if !force {
                            if let Some(prev) = known.get(&key) {
                                if prev == &rec.content_sha256 { skipped += 1; continue; }
                            }
                        }
                        match upsert_session(conn, &rec) {
                            Ok(_) => upserted += 1,
                            Err(e) => { errors += 1; eprintln!("[recall scan] upsert err {:?}: {}", path, e); }
                        }
                    }
                    Err(e) => { errors += 1; eprintln!("[recall scan] parse err {:?}: {}", path, e); }
                }
            }
        } else {
            println!("[recall scan] no Claude root: {}", root.display());
        }
    }

    if provider_filter == "all" || provider_filter == "codex" {
        // Primary: walk ~/.codex/sessions/**/rollout-*.jsonl (modern Codex, one file = one session)
        let sessions_root = codex_sessions_root();
        let title_map = load_codex_session_index();
        let mut rollout_session_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

        if sessions_root.is_dir() {
            for entry in walkdir::WalkDir::new(&sessions_root).into_iter().filter_map(|e| e.ok()) {
                if !entry.file_type().is_file() { continue; }
                let path = entry.path();
                let fname = match path.file_name().and_then(|s| s.to_str()) { Some(n) => n, None => continue };
                if !fname.starts_with("rollout-") || !fname.ends_with(".jsonl") { continue; }
                scanned += 1;
                match parse_codex_rollout(path, &title_map) {
                    Ok(rec) => {
                        rollout_session_ids.insert(rec.session_id.clone());
                        let key = (rec.provider.clone(), rec.session_id.clone());
                        if !force {
                            if let Some(prev) = known.get(&key) {
                                if prev == &rec.content_sha256 { skipped += 1; continue; }
                            }
                        }
                        match upsert_session(conn, &rec) {
                            Ok(_) => upserted += 1,
                            Err(e) => { errors += 1; eprintln!("[recall scan] codex rollout upsert err {:?}: {}", path, e); }
                        }
                    }
                    Err(e) => { errors += 1; eprintln!("[recall scan] codex rollout parse err {:?}: {}", path, e); }
                }
            }
        } else {
            println!("[recall scan] no Codex sessions root: {}", sessions_root.display());
        }

        // Fallback: history.jsonl for session_ids not in sessions/ (legacy / pre-rollout Codex)
        let codex_history = codex_history_path();
        if codex_history.is_file() {
            match parse_codex_history(&codex_history) {
                Ok(records) => {
                    for rec in records {
                        if rollout_session_ids.contains(&rec.session_id) { continue; }
                        scanned += 1;
                        let key = (rec.provider.clone(), rec.session_id.clone());
                        if !force {
                            if let Some(prev) = known.get(&key) {
                                if prev == &rec.content_sha256 { skipped += 1; continue; }
                            }
                        }
                        match upsert_session(conn, &rec) {
                            Ok(_) => upserted += 1,
                            Err(e) => { errors += 1; eprintln!("[recall scan] codex history upsert err {}: {}", rec.session_id, e); }
                        }
                    }
                }
                Err(e) => { errors += 1; eprintln!("[recall scan] codex history parse err: {}", e); }
            }
        }
    }

    rebuild_edges(conn)?;
    println!("[recall scan] DONE scanned={} upserted={} skipped={} err={}", scanned, upserted, skipped, errors);
    Ok(())
}

fn parse_claude_jsonl(path: &Path) -> Result<SessionRow> {
    let body = std::fs::read_to_string(path).context("read jsonl")?;
    let sha = sha256_hex(body.as_bytes());
    let file_size = body.len() as i64;

    let mut session_id = String::new();
    let mut cwd: Option<String> = None;
    let mut title: Option<String> = None;
    let mut first_prompt: Option<String> = None;
    let mut last_prompt: Option<String> = None;
    let mut first_ts: Option<DateTime<Utc>> = None;
    let mut last_ts: Option<DateTime<Utc>> = None;
    let mut user_msg_count = 0i32;
    let mut asst_msg_count = 0i32;
    let mut last_user_content: Option<String> = None;

    for line in body.lines() {
        if line.trim().is_empty() { continue; }
        let v: Value = match serde_json::from_str(line) { Ok(v) => v, Err(_) => continue };
        let t = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
        if session_id.is_empty() {
            if let Some(s) = v.get("sessionId").and_then(|s| s.as_str()) { session_id = s.to_string(); }
        }
        if cwd.is_none() {
            if let Some(c) = v.get("cwd").and_then(|s| s.as_str()) { cwd = Some(c.to_string()); }
        }
        if let Some(ts_str) = v.get("timestamp").and_then(|s| s.as_str()) {
            if let Ok(ts) = DateTime::parse_from_rfc3339(ts_str) {
                let utc = ts.with_timezone(&Utc);
                if first_ts.map_or(true, |f| utc < f) { first_ts = Some(utc); }
                if last_ts.map_or(true, |f| utc > f) { last_ts = Some(utc); }
            }
        }
        match t {
            "user" => {
                if v.get("isMeta").and_then(|b| b.as_bool()).unwrap_or(false) { continue; }
                let content = match v.get("message").and_then(|m| m.get("content")) {
                    Some(Value::String(s)) => s.clone(),
                    Some(Value::Array(arr)) => arr.iter()
                        .filter_map(|i| i.get("text").and_then(|t| t.as_str()).map(|s| s.to_string()))
                        .collect::<Vec<_>>().join("\n"),
                    _ => continue,
                };
                if content.is_empty() { continue; }
                if content.starts_with("<command-") || content.starts_with("<local-command-") { continue; }
                user_msg_count += 1;
                if first_prompt.is_none() { first_prompt = Some(truncate_chars(&content, 4000)); }
                last_user_content = Some(content);
            }
            "assistant" => { asst_msg_count += 1; }
            "ai-title" => {
                if let Some(t2) = v.get("aiTitle").and_then(|s| s.as_str()) { title = Some(t2.to_string()); }
            }
            "last-prompt" => {
                if let Some(lp) = v.get("lastPrompt").and_then(|s| s.as_str()) { last_prompt = Some(truncate_chars(lp, 4000)); }
            }
            _ => {}
        }
    }
    if last_prompt.is_none() {
        if let Some(c) = last_user_content { last_prompt = Some(truncate_chars(&c, 4000)); }
    }
    if session_id.is_empty() { anyhow::bail!("no sessionId in {:?}", path); }

    Ok(SessionRow {
        provider: "claude".into(),
        session_id,
        cwd,
        source_path: path.to_string_lossy().into_owned(),
        title,
        first_prompt,
        last_prompt,
        first_ts: first_ts.map(|t| t.naive_utc()),
        last_ts: last_ts.map(|t| t.naive_utc()),
        message_count: user_msg_count + asst_msg_count,
        user_msg_count,
        asst_msg_count,
        file_size,
        content_sha256: sha,
        body_full: body,
    })
}

/// Modern Codex stores one rollout JSONL per session under
/// ~/.codex/sessions/YYYY/MM/DD/rollout-<ts>-<uuid>.jsonl
/// First line: {"timestamp", "type":"session_meta", "payload":{"id","timestamp","cwd",...}}
/// Then: {"timestamp", "type":"response_item", "payload":{"type":"message","role","content":[{"type":"input_text"|"output_text","text":"..."}]}}
/// (also "event_msg" lines for turn lifecycle — ignored for indexing)
fn parse_codex_rollout(path: &Path, title_map: &std::collections::HashMap<String, String>) -> Result<SessionRow> {
    let raw = std::fs::read_to_string(path).context("read codex rollout")?;
    let sha = sha256_hex(raw.as_bytes());
    let file_size = raw.len() as i64;

    let mut session_id = String::new();
    let mut cwd: Option<String> = None;
    let mut first_ts: Option<DateTime<Utc>> = None;
    let mut last_ts: Option<DateTime<Utc>> = None;
    let mut first_prompt: Option<String> = None;
    let mut last_user_content: Option<String> = None;
    let mut user_msg_count = 0i32;
    let mut asst_msg_count = 0i32;
    // body is the conversation text only (role-tagged), not the raw JSONL — keeps fts5 index focused.
    let mut body_buf = String::new();

    for line in raw.lines() {
        if line.trim().is_empty() { continue; }
        let v: Value = match serde_json::from_str(line) { Ok(v) => v, Err(_) => continue };

        // outer timestamp -> last_ts tracking (first_ts overridden by session_meta.payload.timestamp later if available)
        if let Some(ts_str) = v.get("timestamp").and_then(|s| s.as_str()) {
            if let Ok(ts) = DateTime::parse_from_rfc3339(ts_str) {
                let utc = ts.with_timezone(&Utc);
                if last_ts.map_or(true, |l| utc > l) { last_ts = Some(utc); }
                if first_ts.is_none() { first_ts = Some(utc); }
            }
        }

        let outer_type = v.get("type").and_then(|s| s.as_str()).unwrap_or("");
        let payload = match v.get("payload") { Some(p) => p, None => continue };

        match outer_type {
            "session_meta" => {
                if session_id.is_empty() {
                    if let Some(s) = payload.get("id").and_then(|s| s.as_str()) {
                        session_id = s.to_string();
                    }
                }
                if cwd.is_none() {
                    if let Some(c) = payload.get("cwd").and_then(|s| s.as_str()) {
                        cwd = Some(c.to_string());
                    }
                }
                // payload.timestamp is the canonical session start (outer is when log entry was written)
                if let Some(ts_str) = payload.get("timestamp").and_then(|s| s.as_str()) {
                    if let Ok(ts) = DateTime::parse_from_rfc3339(ts_str) {
                        first_ts = Some(ts.with_timezone(&Utc));
                    }
                }
            }
            "response_item" => {
                let inner_type = payload.get("type").and_then(|s| s.as_str()).unwrap_or("");
                if inner_type != "message" { continue; }
                let role = payload.get("role").and_then(|s| s.as_str()).unwrap_or("");
                // We index user + assistant chat only. "developer"/"system" carries permission boilerplate.
                if role != "user" && role != "assistant" { continue; }
                let content_arr = match payload.get("content") {
                    Some(Value::Array(arr)) => arr,
                    _ => continue,
                };
                let mut content_text = String::new();
                for item in content_arr {
                    if let Some(text) = item.get("text").and_then(|s| s.as_str()) {
                        if !content_text.is_empty() { content_text.push('\n'); }
                        content_text.push_str(text);
                    }
                }
                if content_text.is_empty() { continue; }
                body_buf.push_str(&format!("[{}]\n{}\n---\n", role, content_text));
                if role == "user" {
                    user_msg_count += 1;
                    if first_prompt.is_none() {
                        first_prompt = Some(truncate_chars(&content_text, 4000));
                    }
                    last_user_content = Some(content_text);
                } else {
                    asst_msg_count += 1;
                }
            }
            _ => {}
        }
    }

    // Fallback: derive session_id from filename if session_meta line is missing/corrupted.
    if session_id.is_empty() {
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            // rollout-2026-05-14T05-43-00-019e2314-1bcb-7081-97c2-fda3a9c110fa
            // Last 5 hyphen-separated tokens form a UUID.
            let parts: Vec<&str> = stem.split('-').collect();
            if parts.len() >= 5 {
                let uuid = parts[parts.len() - 5..].join("-");
                if uuid.len() == 36 { session_id = uuid; }
            }
        }
        if session_id.is_empty() {
            anyhow::bail!("no session_meta and unparsable filename: {:?}", path);
        }
    }

    let last_prompt = last_user_content.map(|c| truncate_chars(&c, 4000));
    let title = title_map.get(&session_id).cloned();

    Ok(SessionRow {
        provider: "codex".into(),
        session_id,
        cwd,
        source_path: path.to_string_lossy().into_owned(),
        title,
        first_prompt,
        last_prompt,
        first_ts: first_ts.map(|t| t.naive_utc()),
        last_ts: last_ts.map(|t| t.naive_utc()),
        message_count: user_msg_count + asst_msg_count,
        user_msg_count,
        asst_msg_count,
        file_size,
        content_sha256: sha,
        body_full: body_buf,
    })
}

fn parse_codex_history(path: &Path) -> Result<Vec<SessionRow>> {
    let body = std::fs::read_to_string(path).context("read codex history")?;
    let mut groups: std::collections::HashMap<String, Vec<(i64, String)>> = std::collections::HashMap::new();
    for line in body.lines() {
        if line.trim().is_empty() { continue; }
        let v: Value = match serde_json::from_str(line) { Ok(v) => v, Err(_) => continue };
        let sid = v.get("session_id").and_then(|s| s.as_str()).unwrap_or("").to_string();
        if sid.is_empty() { continue; }
        let ts = v.get("ts").and_then(|t| t.as_i64()).unwrap_or(0);
        let text = v.get("text").and_then(|s| s.as_str()).unwrap_or("").to_string();
        groups.entry(sid).or_default().push((ts, text));
    }
    let source = path.to_string_lossy().into_owned();
    let mut records = Vec::new();
    for (sid, mut msgs) in groups {
        msgs.sort_by_key(|m| m.0);
        msgs.dedup_by(|a, b| a.1 == b.1);
        let first_text = msgs.first().map(|(_, t)| t.clone());
        let last_text = msgs.last().map(|(_, t)| t.clone());
        let first_ts = msgs.first().and_then(|(ts, _)| Utc.timestamp_opt(*ts, 0).single()).map(|t| t.naive_utc());
        let last_ts = msgs.last().and_then(|(ts, _)| Utc.timestamp_opt(*ts, 0).single()).map(|t| t.naive_utc());
        let user_msg_count = msgs.len() as i32;
        let body_full = msgs.iter().map(|(ts, t)| format!("[{}]\n{}", ts, t)).collect::<Vec<_>>().join("\n---\n");
        let sha = sha256_hex(body_full.as_bytes());
        let file_size = body_full.len() as i64;
        records.push(SessionRow {
            provider: "codex".into(),
            session_id: sid,
            cwd: None,
            source_path: source.clone(),
            title: None,
            first_prompt: first_text.map(|s| truncate_chars(&s, 4000)),
            last_prompt: last_text.map(|s| truncate_chars(&s, 4000)),
            first_ts,
            last_ts,
            message_count: user_msg_count,
            user_msg_count,
            asst_msg_count: 0,
            file_size,
            content_sha256: sha,
            body_full,
        });
    }
    Ok(records)
}

fn upsert_session(conn: &mut Connection, rec: &SessionRow) -> Result<()> {
    let tx = conn.transaction()?;
    tx.execute(
        r#"INSERT INTO sessions
            (provider, session_id, cwd, source_path, title, first_prompt, last_prompt,
             first_ts, last_ts, message_count, user_msg_count, asst_msg_count,
             file_size, content_sha256, updated_at)
           VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, CURRENT_TIMESTAMP)
           ON CONFLICT(provider, session_id) DO UPDATE SET
             cwd=excluded.cwd, source_path=excluded.source_path,
             title=excluded.title, first_prompt=excluded.first_prompt, last_prompt=excluded.last_prompt,
             first_ts=excluded.first_ts, last_ts=excluded.last_ts,
             message_count=excluded.message_count, user_msg_count=excluded.user_msg_count, asst_msg_count=excluded.asst_msg_count,
             file_size=excluded.file_size, content_sha256=excluded.content_sha256,
             updated_at=CURRENT_TIMESTAMP"#,
        params![
            rec.provider, rec.session_id, rec.cwd, rec.source_path,
            rec.title, rec.first_prompt, rec.last_prompt,
            rec.first_ts.map(|t| t.to_string()), rec.last_ts.map(|t| t.to_string()),
            rec.message_count, rec.user_msg_count, rec.asst_msg_count,
            rec.file_size, rec.content_sha256,
        ],
    )?;
    let pk: i64 = tx.query_row(
        "SELECT id FROM sessions WHERE provider=?1 AND session_id=?2",
        params![rec.provider, rec.session_id], |r| r.get(0),
    )?;
    tx.execute("DELETE FROM sessions_fts WHERE session_pk=?1", params![pk])?;
    tx.execute(
        "INSERT INTO sessions_fts (session_pk, title, first_prompt, last_prompt, body) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![pk, rec.title.clone().unwrap_or_default(), rec.first_prompt.clone().unwrap_or_default(),
                rec.last_prompt.clone().unwrap_or_default(), rec.body_full],
    )?;
    tx.commit()?;
    Ok(())
}

fn rebuild_edges(conn: &mut Connection) -> Result<()> {
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM edges WHERE kind='same_cwd'", [])?;
    tx.execute(
        r#"INSERT OR IGNORE INTO edges (src_session_pk, dst_session_pk, kind, weight)
           SELECT a.id, b.id, 'same_cwd', 1.0
           FROM sessions a JOIN sessions b
             ON a.cwd IS NOT NULL AND a.cwd = b.cwd AND a.id <> b.id"#,
        [],
    )?;
    tx.commit()?;
    Ok(())
}

// ----- search -----

fn cmd_search(conn: &Connection, keyword: &str, n: usize, provider: Option<&str>) -> Result<()> {
    let mut sql = String::from(
        r#"SELECT s.provider, s.session_id, s.cwd, s.title, s.last_prompt, s.last_ts,
                  snippet(sessions_fts, 4, '<<', '>>', '...', 12) AS excerpt
           FROM sessions_fts
           JOIN sessions s ON s.id = sessions_fts.session_pk
           WHERE sessions_fts MATCH ?1"#,
    );
    if provider.is_some() { sql.push_str(" AND s.provider = ?2"); }
    sql.push_str(" ORDER BY rank LIMIT ");
    sql.push_str(&n.to_string());
    let mut stmt = conn.prepare(&sql)?;
    let pattern = fts5_escape(keyword);
    let rows: Vec<SearchRow> = if let Some(p) = provider {
        stmt.query_map(params![pattern, p], extract_search_row)?.collect::<rusqlite::Result<Vec<_>>>()?
    } else {
        stmt.query_map(params![pattern], extract_search_row)?.collect::<rusqlite::Result<Vec<_>>>()?
    };
    print_search_results(&rows, keyword);
    Ok(())
}

fn extract_search_row(r: &rusqlite::Row) -> rusqlite::Result<SearchRow> {
    Ok(SearchRow {
        provider: r.get(0)?,
        session_id: r.get(1)?,
        cwd: r.get(2)?,
        title: r.get(3)?,
        last_prompt: r.get(4)?,
        last_ts: r.get(5)?,
        excerpt: r.get(6)?,
    })
}

struct SearchRow {
    provider: String,
    session_id: String,
    cwd: Option<String>,
    title: Option<String>,
    last_prompt: Option<String>,
    last_ts: Option<String>,
    excerpt: String,
}

fn fts5_escape(q: &str) -> String {
    let cleaned: String = q.chars().filter(|c| *c != '"').collect();
    format!("\"{}\"", cleaned)
}

fn print_search_results(rows: &[SearchRow], keyword: &str) {
    if rows.is_empty() { println!("no matches for '{}'", keyword); return; }
    println!("{:<7} {:<10} {:<19} {:<28}  {}", "PROV", "SID_8", "LAST_TS", "TITLE", "EXCERPT");
    println!("{}", "-".repeat(160));
    for r in rows {
        let short = &r.session_id[..8.min(r.session_id.len())];
        let title_t: String = r.title.clone().unwrap_or_default().chars().take(28).collect();
        let ts = r.last_ts.clone().unwrap_or_default();
        let excerpt = r.excerpt.replace('\n', " ").chars().take(80).collect::<String>();
        println!("{:<7} {:<10} {:<19} {:<28}  {}", r.provider, short, ts, title_t, excerpt);
        let _ = (&r.cwd, &r.last_prompt);
    }
    println!("\n{} matches for '{}'", rows.len(), keyword);
}

// ----- show -----

fn cmd_show(conn: &Connection, prefix: &str) -> Result<()> {
    let mut stmt = conn.prepare(
        r#"SELECT provider, session_id, cwd, source_path, title, first_prompt, last_prompt,
                  first_ts, last_ts, message_count, user_msg_count, asst_msg_count, file_size
           FROM sessions WHERE session_id LIKE ?1 ORDER BY last_ts DESC LIMIT 5"#,
    )?;
    let pat = format!("{}%", prefix);
    let rows = stmt.query_map(params![pat], |r| {
        Ok((
            r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?,
            r.get::<_, String>(3)?, r.get::<_, Option<String>>(4)?, r.get::<_, Option<String>>(5)?,
            r.get::<_, Option<String>>(6)?, r.get::<_, Option<String>>(7)?, r.get::<_, Option<String>>(8)?,
            r.get::<_, i32>(9)?, r.get::<_, i32>(10)?, r.get::<_, i32>(11)?, r.get::<_, i64>(12)?,
        ))
    })?.collect::<rusqlite::Result<Vec<_>>>()?;
    if rows.is_empty() { eprintln!("no session for prefix '{}'", prefix); std::process::exit(1); }
    for (prov, sid, cwd, src, title, fp, lp, fts, lts, mc, umc, amc, fs) in rows {
        println!("------------------------------------------");
        println!("provider   : {}", prov);
        println!("session_id : {}", sid);
        println!("cwd        : {}", cwd.unwrap_or_default());
        println!("source     : {}", src);
        println!("title      : {}", title.unwrap_or_default());
        println!("first_ts   : {}", fts.unwrap_or_default());
        println!("last_ts    : {}", lts.unwrap_or_default());
        println!("messages   : total={} user={} asst={}", mc, umc, amc);
        println!("file_size  : {} bytes", fs);
        if let Some(p) = fp { println!("\n--- first prompt ---\n{}\n", p); }
        if let Some(p) = lp { println!("--- last prompt ---\n{}", p); }
    }
    Ok(())
}

// ----- resume -----

fn cmd_resume(conn: &Connection, query: &str) -> Result<()> {
    let sid_pat = format!("{}%", query);
    let mut stmt = conn.prepare(
        "SELECT provider, session_id, cwd FROM sessions WHERE session_id LIKE ?1 ORDER BY last_ts DESC LIMIT 5",
    )?;
    let mut hits: Vec<(String, String, Option<String>)> = stmt
        .query_map(params![sid_pat], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if hits.is_empty() {
        let mut fts = conn.prepare(
            r#"SELECT s.provider, s.session_id, s.cwd
               FROM sessions_fts JOIN sessions s ON s.id = sessions_fts.session_pk
               WHERE sessions_fts MATCH ?1 ORDER BY rank LIMIT 5"#,
        )?;
        let pat = fts5_escape(query);
        hits = fts.query_map(params![pat], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
    }
    if hits.is_empty() { eprintln!("[recall resume] no match for '{}'", query); std::process::exit(1); }
    if hits.len() > 1 {
        println!("multiple matches for '{}':\n", query);
        for (p, sid, cwd) in &hits {
            println!("  [{}]  {}  cwd={}", p, &sid[..8.min(sid.len())], cwd.clone().unwrap_or_default());
        }
        eprintln!("\nUse a session_id prefix to disambiguate.");
        std::process::exit(1);
    }
    let (prov, sid, cwd) = &hits[0];
    println!("matched   : {} :: {}", prov, sid);
    if let Some(c) = cwd { println!("cwd       : {}", c); }
    println!();
    println!("To resume this session, paste the following one-liner into your current CLI:");
    println!();
    println!("    /resume {}", sid);
    println!();
    println!("(Both claude and codex accept `/resume <session_id>` as an in-session slash command.)");
    Ok(())
}

// ----- stats / related -----

fn cmd_stats(conn: &Connection) -> Result<()> {
    let mut stmt = conn.prepare(
        r#"SELECT provider, COUNT(*), SUM(message_count), SUM(file_size)/1024, MAX(last_ts)
           FROM sessions GROUP BY provider ORDER BY provider"#,
    )?;
    println!("{:<10} {:>9} {:>10} {:>12}  {}", "PROV", "SESSIONS", "MESSAGES", "SIZE_KB", "LAST_ACTIVITY");
    println!("{}", "-".repeat(60));
    let rows = stmt.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, Option<i64>>(2)?, r.get::<_, Option<i64>>(3)?, r.get::<_, Option<String>>(4)?))
    })?;
    for row in rows {
        let (p, n, m, kb, last) = row?;
        println!("{:<10} {:>9} {:>10} {:>12}  {}", p, n, m.unwrap_or(0), kb.unwrap_or(0), last.unwrap_or_default());
    }
    Ok(())
}

fn cmd_related(conn: &Connection, prefix: &str, n: usize) -> Result<()> {
    let pat = format!("{}%", prefix);
    let pk: i64 = conn.query_row(
        "SELECT id FROM sessions WHERE session_id LIKE ?1 ORDER BY last_ts DESC LIMIT 1",
        params![pat], |r| r.get(0),
    ).context("session not found")?;
    let mut stmt = conn.prepare(&format!(
        r#"SELECT DISTINCT s.provider, s.session_id, s.cwd, s.title, s.last_ts, e.kind
           FROM edges e JOIN sessions s ON s.id = e.dst_session_pk
           WHERE e.src_session_pk = ?1 ORDER BY s.last_ts DESC LIMIT {}"#, n
    ))?;
    let rows = stmt.query_map(params![pk], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?, r.get::<_, Option<String>>(3)?, r.get::<_, Option<String>>(4)?, r.get::<_, String>(5)?))
    })?.collect::<rusqlite::Result<Vec<_>>>()?;
    if rows.is_empty() { println!("no related sessions found."); return Ok(()); }
    println!("{:<7} {:<10} {:<19} {:<12} {}", "PROV", "SID_8", "LAST_TS", "EDGE", "TITLE");
    println!("{}", "-".repeat(110));
    for (p, sid, _cwd, title, lts, kind) in rows {
        let short = &sid[..8.min(sid.len())];
        let title_t: String = title.unwrap_or_default().chars().take(40).collect();
        println!("{:<7} {:<10} {:<19} {:<12} {}", p, short, lts.unwrap_or_default(), kind, title_t);
    }
    Ok(())
}

// ----- daemon -----

fn cmd_daemon(action: DaemonAction) -> Result<()> {
    let bin = std::env::current_exe().context("locate current exe")?;
    let bin_str = bin.to_string_lossy().into_owned();
    match action {
        DaemonAction::Install { interval_min } => daemon_install(&bin_str, interval_min),
        DaemonAction::Uninstall => daemon_uninstall(),
        DaemonAction::Status => daemon_status(),
    }
}

#[cfg(target_os = "windows")]
fn daemon_install(bin: &str, interval_min: u32) -> Result<()> {
    let tr = format!("\"{}\" scan", bin);
    let status = std::process::Command::new("schtasks")
        .args(["/Create", "/TN", "recall-scan", "/TR", &tr, "/SC", "MINUTE", "/MO", &interval_min.to_string(), "/F"])
        .status().context("invoke schtasks")?;
    if !status.success() { anyhow::bail!("schtasks /Create failed"); }
    println!("[recall daemon] Windows Scheduled Task 'recall-scan' registered (every {} min)", interval_min);
    Ok(())
}

#[cfg(target_os = "windows")]
fn daemon_uninstall() -> Result<()> {
    let _ = std::process::Command::new("schtasks").args(["/Delete", "/TN", "recall-scan", "/F"]).status();
    println!("[recall daemon] Scheduled Task 'recall-scan' removed (if it existed)");
    Ok(())
}

#[cfg(target_os = "windows")]
fn daemon_status() -> Result<()> {
    let output = std::process::Command::new("schtasks").args(["/Query", "/TN", "recall-scan"]).output()?;
    print!("{}", String::from_utf8_lossy(&output.stdout));
    eprint!("{}", String::from_utf8_lossy(&output.stderr));
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn daemon_install(bin: &str, interval_min: u32) -> Result<()> {
    use std::io::Write;
    let new_line = format!("*/{} * * * * {} scan >> /tmp/recall-scan.log 2>&1\n", interval_min, bin);
    let current = std::process::Command::new("crontab").arg("-l").output().ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    let filtered: String = current.lines().filter(|l| !l.contains("recall scan")).map(|l| format!("{}\n", l)).collect();
    let merged = format!("{}{}", filtered, new_line);
    let mut child = std::process::Command::new("crontab").arg("-")
        .stdin(std::process::Stdio::piped()).spawn().context("spawn crontab")?;
    child.stdin.as_mut().unwrap().write_all(merged.as_bytes())?;
    let status = child.wait()?;
    if !status.success() { anyhow::bail!("crontab failed"); }
    println!("[recall daemon] crontab entry registered (every {} min): {}", interval_min, new_line.trim_end());
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn daemon_uninstall() -> Result<()> {
    use std::io::Write;
    let current = std::process::Command::new("crontab").arg("-l").output().ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    let filtered: String = current.lines().filter(|l| !l.contains("recall scan")).map(|l| format!("{}\n", l)).collect();
    let mut child = std::process::Command::new("crontab").arg("-")
        .stdin(std::process::Stdio::piped()).spawn()?;
    child.stdin.as_mut().unwrap().write_all(filtered.as_bytes())?;
    let _ = child.wait()?;
    println!("[recall daemon] crontab 'recall scan' entries removed");
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn daemon_status() -> Result<()> {
    let output = std::process::Command::new("crontab").arg("-l").output()?;
    let s = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = s.lines().filter(|l| l.contains("recall scan")).collect();
    if lines.is_empty() { println!("[recall daemon] no crontab entry for recall scan"); }
    else { for l in lines { println!("{}", l); } }
    Ok(())
}
