//! recall — find any past Claude Code / Codex session by fuzzy memory.
//!
//! Single-node, local-first. SQLite (FTS5) + optional embedding-based ANN.
//! No telemetry, no cloud sync, no credentials baked in.

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

CREATE TABLE IF NOT EXISTS embeddings (
    session_pk      INTEGER PRIMARY KEY,
    model           TEXT    NOT NULL,
    dim             INTEGER NOT NULL,
    vec             BLOB    NOT NULL,
    embedded_at     TEXT    NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (session_pk) REFERENCES sessions(id) ON DELETE CASCADE
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
    /// 세션 resume — Claude 면 `claude --resume`, Codex 면 `codex resume`
    Resume {
        query: String,
        #[arg(long)]
        dry_run: bool,
    },
    /// 통계
    Stats,
    /// 같은 cwd + 키워드 매칭으로 연관 세션 찾기 (그래프 1-hop)
    Related {
        session_id_prefix: String,
        #[arg(long, short, default_value_t = 10)]
        n: usize,
    },
    /// 임베딩 생성 (사용자 API key 필요. 자세한 내용은 README 참조)
    Embed {
        #[arg(long, default_value = "all")]
        provider: String,
        #[arg(long)]
        force: bool,
    },
    /// 의미 기반 검색 (임베딩 + cosine top-K). embed 선행 필요.
    Semantic {
        keyword: String,
        #[arg(long, short, default_value_t = 10)]
        n: usize,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let db_path = cli.db.unwrap_or_else(default_db_path);
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let mut conn = Connection::open(&db_path).context("open SQLite DB")?;
    conn.execute_batch(SCHEMA_SQL).context("apply schema")?;

    match cli.cmd {
        Cmd::Init => {
            println!("[recall] DB ready at {}", db_path.display());
        }
        Cmd::Scan { provider, force } => cmd_scan(&mut conn, &provider, force).await?,
        Cmd::Search { keyword, n, provider } => cmd_search(&conn, &keyword, n, provider.as_deref())?,
        Cmd::Show { session_id_prefix } => cmd_show(&conn, &session_id_prefix)?,
        Cmd::Resume { query, dry_run } => cmd_resume(&conn, &query, dry_run)?,
        Cmd::Stats => cmd_stats(&conn)?,
        Cmd::Related { session_id_prefix, n } => cmd_related(&conn, &session_id_prefix, n)?,
        Cmd::Embed { provider, force } => cmd_embed(&mut conn, &provider, force).await?,
        Cmd::Semantic { keyword, n } => cmd_semantic(&conn, &keyword, n).await?,
    }
    Ok(())
}

fn default_db_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".recall")
        .join("recall.db")
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

async fn cmd_scan(conn: &mut Connection, provider_filter: &str, force: bool) -> Result<()> {
    println!("[recall scan] provider={} force={}", provider_filter, force);

    let mut known: std::collections::HashMap<(String, String), String> = std::collections::HashMap::new();
    {
        let mut stmt = conn.prepare("SELECT provider, session_id, content_sha256 FROM sessions")?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?))
        })?;
        for row in rows {
            let (p, s, h) = row?;
            if let Some(h) = h {
                known.insert((p, s), h);
            }
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
        let codex = codex_history_path();
        if codex.is_file() {
            match parse_codex_history(&codex) {
                Ok(records) => {
                    for rec in records {
                        scanned += 1;
                        let key = (rec.provider.clone(), rec.session_id.clone());
                        if !force {
                            if let Some(prev) = known.get(&key) {
                                if prev == &rec.content_sha256 { skipped += 1; continue; }
                            }
                        }
                        match upsert_session(conn, &rec) {
                            Ok(_) => upserted += 1,
                            Err(e) => { errors += 1; eprintln!("[recall scan] codex upsert err {}: {}", rec.session_id, e); }
                        }
                    }
                }
                Err(e) => { errors += 1; eprintln!("[recall scan] codex parse err: {}", e); }
            }
        } else {
            println!("[recall scan] no Codex history at {}", codex.display());
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
                    Some(Value::Array(arr)) => arr.iter().filter_map(|i| i.get("text").and_then(|t| t.as_str()).map(|s| s.to_string())).collect::<Vec<_>>().join("\n"),
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
    if session_id.is_empty() {
        anyhow::bail!("no sessionId in {:?}", path);
    }

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
        params![rec.provider, rec.session_id],
        |r| r.get(0),
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

fn cmd_search(conn: &Connection, keyword: &str, n: usize, provider: Option<&str>) -> Result<()> {
    let mut sql = String::from(
        r#"SELECT s.provider, s.session_id, s.cwd, s.title, s.last_prompt, s.last_ts,
                  snippet(sessions_fts, 4, '«', '»', '…', 12) AS excerpt
           FROM sessions_fts
           JOIN sessions s ON s.id = sessions_fts.session_pk
           WHERE sessions_fts MATCH ?1"#,
    );
    if provider.is_some() { sql.push_str(" AND s.provider = ?2"); }
    sql.push_str(" ORDER BY rank LIMIT ");
    sql.push_str(&n.to_string());
    let mut stmt = conn.prepare(&sql)?;
    let pattern = fts5_escape(keyword);
    let rows = if let Some(p) = provider {
        stmt.query_map(params![pattern, p], extract_search_row)?.collect::<Result<Vec<_>, _>>()?
    } else {
        stmt.query_map(params![pattern], extract_search_row)?.collect::<Result<Vec<_>, _>>()?
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
    // 사용자 입력을 phrase 로 감싸서 FTS5 syntax 보호
    let cleaned: String = q.chars().filter(|c| *c != '"').collect();
    format!("\"{}\"", cleaned)
}

fn print_search_results(rows: &[SearchRow], keyword: &str) {
    if rows.is_empty() {
        println!("no matches for '{}'", keyword);
        return;
    }
    println!("{:<7} {:<10} {:<19} {:<28}  {}", "PROV", "SID_8", "LAST_TS", "TITLE", "EXCERPT");
    println!("{}", "-".repeat(160));
    for r in rows {
        let short = &r.session_id[..8.min(r.session_id.len())];
        let title_t: String = r.title.clone().unwrap_or_default().chars().take(28).collect();
        let ts = r.last_ts.clone().unwrap_or_default();
        let excerpt = r.excerpt.replace('\n', " ").chars().take(80).collect::<String>();
        println!("{:<7} {:<10} {:<19} {:<28}  {}", r.provider, short, ts, title_t, excerpt);
        let _ = r.cwd; let _ = r.last_prompt;
    }
    println!("\n{} matches for '{}'", rows.len(), keyword);
}

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
    })?.collect::<Result<Vec<_>, _>>()?;
    if rows.is_empty() {
        eprintln!("no session for prefix '{}'", prefix);
        std::process::exit(1);
    }
    for (prov, sid, cwd, src, title, fp, lp, fts, lts, mc, umc, amc, fs) in rows {
        println!("──────────────────────────────────────────");
        println!("provider   : {}", prov);
        println!("session_id : {}", sid);
        println!("cwd        : {}", cwd.unwrap_or_default());
        println!("source     : {}", src);
        println!("title      : {}", title.unwrap_or_default());
        println!("first_ts   : {}", fts.unwrap_or_default());
        println!("last_ts    : {}", lts.unwrap_or_default());
        println!("messages   : total={} user={} asst={}", mc, umc, amc);
        println!("file_size  : {} bytes", fs);
        if let Some(p) = fp { println!("\n── first prompt ──\n{}\n", p); }
        if let Some(p) = lp { println!("── last prompt ──\n{}", p); }
    }
    Ok(())
}

fn cmd_resume(conn: &Connection, query: &str, dry_run: bool) -> Result<()> {
    let sid_pat = format!("{}%", query);
    // 1) session_id prefix 매칭
    let mut stmt = conn.prepare(
        "SELECT provider, session_id, cwd FROM sessions WHERE session_id LIKE ?1 ORDER BY last_ts DESC LIMIT 5",
    )?;
    let mut hits: Vec<(String, String, Option<String>)> = stmt
        .query_map(params![sid_pat], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    // 2) FTS 키워드 매칭
    if hits.is_empty() {
        let mut fts = conn.prepare(
            r#"SELECT s.provider, s.session_id, s.cwd
               FROM sessions_fts JOIN sessions s ON s.id = sessions_fts.session_pk
               WHERE sessions_fts MATCH ?1 ORDER BY rank LIMIT 5"#,
        )?;
        let pat = fts5_escape(query);
        hits = fts.query_map(params![pat], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?)))?
            .collect::<Result<Vec<_>, _>>()?;
    }
    if hits.is_empty() {
        eprintln!("[recall resume] no match for '{}'", query);
        std::process::exit(1);
    }
    if hits.len() > 1 {
        println!("multiple matches for '{}':\n", query);
        for (p, sid, cwd) in &hits {
            println!("  [{}]  {}  cwd={}", p, &sid[..8.min(sid.len())], cwd.clone().unwrap_or_default());
        }
        eprintln!("\nUse a session_id prefix to disambiguate.");
        std::process::exit(1);
    }
    let (prov, sid, cwd) = &hits[0];
    let cmd_str = match prov.as_str() {
        "claude" => format!("claude --resume {}", sid),
        "codex" => format!("codex resume {}", sid),
        _ => { eprintln!("unknown provider {}", prov); std::process::exit(2); }
    };
    println!("[recall resume] {} :: {}", prov, sid);
    if dry_run {
        println!("[dry-run] would run: {}", cmd_str);
        if let Some(c) = cwd { println!("[dry-run]   in cwd: {}", c); }
        return Ok(());
    }
    let parts: Vec<&str> = cmd_str.split_whitespace().collect();
    let mut cmd = std::process::Command::new(parts[0]);
    cmd.args(&parts[1..]);
    if let Some(c) = cwd { cmd.current_dir(c); }
    let status = cmd.status()?;
    std::process::exit(status.code().unwrap_or(1));
}

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
    })?.collect::<Result<Vec<_>, _>>()?;
    if rows.is_empty() {
        println!("no related sessions found.");
        return Ok(());
    }
    println!("{:<7} {:<10} {:<19} {:<12} {}", "PROV", "SID_8", "LAST_TS", "EDGE", "TITLE");
    println!("{}", "-".repeat(110));
    for (p, sid, _cwd, title, lts, kind) in rows {
        let short = &sid[..8.min(sid.len())];
        let title_t: String = title.unwrap_or_default().chars().take(40).collect();
        println!("{:<7} {:<10} {:<19} {:<12} {}", p, short, lts.unwrap_or_default(), kind, title_t);
    }
    Ok(())
}

// ────────── Embeddings (Phase 2 — requires user API key) ──────────

#[derive(Deserialize, Default)]
struct RecallConfig {
    #[serde(default)]
    embedding: EmbeddingConfig,
}

#[derive(Deserialize, Default)]
struct EmbeddingConfig {
    /// "openai" / "voyage" / (extend later)
    #[serde(default)]
    provider: String,
    /// e.g. "text-embedding-3-small"
    #[serde(default)]
    model: String,
    /// env var name that holds the API key
    #[serde(default)]
    api_key_env: String,
}

fn load_config() -> RecallConfig {
    let path = home().join(".recall").join("config.toml");
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| toml::from_str(&s).ok())
        .unwrap_or_default()
}

async fn cmd_embed(conn: &mut Connection, provider_filter: &str, force: bool) -> Result<()> {
    let cfg = load_config();
    if cfg.embedding.provider.is_empty() {
        eprintln!("No embedding provider configured.");
        eprintln!("Create ~/.recall/config.toml with:");
        eprintln!("[embedding]\nprovider = \"openai\"\nmodel = \"text-embedding-3-small\"\napi_key_env = \"OPENAI_API_KEY\"");
        std::process::exit(2);
    }
    let api_key = std::env::var(&cfg.embedding.api_key_env)
        .with_context(|| format!("env var {} not set", cfg.embedding.api_key_env))?;
    let model = cfg.embedding.model.clone();

    let mut stmt = conn.prepare(
        r#"SELECT s.id, s.first_prompt, s.last_prompt, s.title
           FROM sessions s LEFT JOIN embeddings e ON e.session_pk = s.id
           WHERE (?1='all' OR s.provider=?1) AND (?2 OR e.session_pk IS NULL)"#,
    )?;
    let force_i: i64 = if force { 1 } else { 0 };
    let targets: Vec<(i64, String)> = stmt
        .query_map(params![provider_filter, force_i], |r| {
            let id: i64 = r.get(0)?;
            let fp: Option<String> = r.get(1)?;
            let lp: Option<String> = r.get(2)?;
            let t: Option<String> = r.get(3)?;
            let text = [t.as_deref().unwrap_or(""), fp.as_deref().unwrap_or(""), lp.as_deref().unwrap_or("")].join("\n").trim().to_string();
            Ok((id, text))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    println!("[recall embed] {} sessions to embed (model={})", targets.len(), model);
    let client = reqwest::Client::new();
    for (i, (pk, text)) in targets.iter().enumerate() {
        if text.is_empty() { continue; }
        match embed_text(&client, &cfg.embedding.provider, &model, &api_key, text).await {
            Ok(vec) => {
                let dim = vec.len() as i64;
                let blob = vec_to_blob(&vec);
                conn.execute(
                    r#"INSERT INTO embeddings (session_pk, model, dim, vec)
                       VALUES (?1, ?2, ?3, ?4)
                       ON CONFLICT(session_pk) DO UPDATE SET model=?2, dim=?3, vec=?4, embedded_at=CURRENT_TIMESTAMP"#,
                    params![pk, model, dim, blob],
                )?;
                if (i + 1) % 10 == 0 { println!("[recall embed] {}/{}", i + 1, targets.len()); }
            }
            Err(e) => eprintln!("[recall embed] err pk={}: {}", pk, e),
        }
    }
    println!("[recall embed] DONE");
    Ok(())
}

async fn embed_text(client: &reqwest::Client, provider: &str, model: &str, api_key: &str, text: &str) -> Result<Vec<f32>> {
    match provider {
        "openai" => {
            #[derive(Serialize)]
            struct Req<'a> { input: &'a str, model: &'a str }
            #[derive(Deserialize)]
            struct Resp { data: Vec<RespItem> }
            #[derive(Deserialize)]
            struct RespItem { embedding: Vec<f32> }
            let r: Resp = client
                .post("https://api.openai.com/v1/embeddings")
                .bearer_auth(api_key)
                .json(&Req { input: text, model })
                .send().await?.error_for_status()?.json().await?;
            r.data.into_iter().next().map(|i| i.embedding).context("empty embedding response")
        }
        other => anyhow::bail!("provider {} not implemented; see README to extend", other),
    }
}

fn vec_to_blob(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for x in v { out.extend_from_slice(&x.to_le_bytes()); }
    out
}

fn blob_to_vec(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() { return 0.0; }
    let mut dot = 0.0f32; let mut na = 0.0f32; let mut nb = 0.0f32;
    for (x, y) in a.iter().zip(b.iter()) { dot += x * y; na += x * x; nb += y * y; }
    if na == 0.0 || nb == 0.0 { 0.0 } else { dot / (na.sqrt() * nb.sqrt()) }
}

async fn cmd_semantic(conn: &Connection, keyword: &str, n: usize) -> Result<()> {
    let cfg = load_config();
    if cfg.embedding.provider.is_empty() {
        eprintln!("Configure ~/.recall/config.toml first. See README.");
        std::process::exit(2);
    }
    let api_key = std::env::var(&cfg.embedding.api_key_env)
        .with_context(|| format!("env var {} not set", cfg.embedding.api_key_env))?;
    let client = reqwest::Client::new();
    let qv = embed_text(&client, &cfg.embedding.provider, &cfg.embedding.model, &api_key, keyword).await?;

    let mut stmt = conn.prepare("SELECT session_pk, vec FROM embeddings")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut scored: Vec<(i64, f32)> = rows.iter()
        .map(|(pk, blob)| (*pk, cosine(&qv, &blob_to_vec(blob))))
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(n);

    println!("{:<7} {:<10} {:<19} {:<28}  {:>6}", "PROV", "SID_8", "LAST_TS", "TITLE", "SCORE");
    println!("{}", "-".repeat(90));
    for (pk, score) in &scored {
        let row: rusqlite::Result<(String, String, Option<String>, Option<String>)> = conn.query_row(
            "SELECT provider, session_id, last_ts, title FROM sessions WHERE id=?1",
            params![pk], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        );
        if let Ok((p, sid, lts, title)) = row {
            let short = &sid[..8.min(sid.len())];
            let title_t: String = title.unwrap_or_default().chars().take(28).collect();
            println!("{:<7} {:<10} {:<19} {:<28}  {:>6.3}", p, short, lts.unwrap_or_default(), title_t, score);
        }
    }
    Ok(())
}
