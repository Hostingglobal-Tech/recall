# recall

> **Find any past Claude Code / Codex session by fuzzy memory.**
> Local-first. SQLite (FTS5) + optional vector search. One-click `claude --resume` / `codex resume`.

`claude --resume` and `codex resume` both ship a picker, but once you have hundreds of sessions, you can't actually *find* the one you want. You vaguely remember **the topic**, not the session id. You never renamed it. You never forked it. `recall` is for that moment.

```
$ recall search "deno ws auth header"
PROV    SID_8      LAST_TS              TITLE                          EXCERPT
claude  3cca0676   2026-05-20 10:00     wire auth header to deno…      …pass the «auth» token via WebSocket «header»…
codex   019e3891   2026-05-18 10:08                                    …I'm working on Deno WS «auth header» mid…

$ recall resume "deno ws auth header"
[recall resume] claude :: 3cca0676-1106-4c5a-8f1f-1080ad72e4cd
# launches `claude --resume 3cca0676-…` in the original cwd
```

## Why

The point of CLI agents is to start exactly where you left off. But "where you left off" is identified by a UUID you've never seen and a picker that doesn't know what you were *trying* to do. `recall` indexes everything locally and lets you find sessions the way you actually remember them — by a fuzzy keyword.

- **Local-first.** Your sessions never leave your machine. No cloud sync, no telemetry.
- **Two providers in one place.** Indexes Claude Code (`~/.claude/projects/**/*.jsonl`) + Codex (`~/.codex/history.jsonl`) together.
- **FTS5 full-text + optional vector search.** SQLite's FTS5 covers exact phrases; bring your own embedding API key for semantic ("by meaning, not by word") matches.
- **One-click resume.** `recall resume <id|keyword>` dispatches to the right CLI (`claude --resume` or `codex resume`) in the right cwd.

## Install

### Build from source (Rust 1.74+)

```bash
git clone https://github.com/Hostingglobal-Tech/recall.git
cd recall
cargo build --release
# binary at: target/release/recall (Linux/macOS) or target\release\recall.exe (Windows)
cp target/release/recall ~/.local/bin/   # or wherever you keep CLIs
```

### First run

```bash
recall init
recall scan
```

`init` creates `~/.recall/recall.db`. `scan` walks `~/.claude/projects/` and `~/.codex/history.jsonl` and indexes everything into SQLite.

## Commands

| Command | What it does |
|---|---|
| `recall init` | create `~/.recall/recall.db` (idempotent) |
| `recall scan [--provider claude\|codex\|all] [--force]` | walk local session files, upsert into DB |
| `recall search "<keyword>"` | FTS5 full-text search across title / prompts / body |
| `recall semantic "<keyword>"` | embedding-based cosine top-K (requires API key, see below) |
| `recall show <session_id_prefix>` | show metadata + first/last prompt |
| `recall resume <id\|keyword> [--dry-run]` | dispatch to `claude --resume` or `codex resume` in the right cwd |
| `recall related <session_id_prefix>` | sessions sharing the same `cwd` (1-hop graph) |
| `recall embed [--provider all] [--force]` | embed unembedded sessions (requires API key) |
| `recall stats` | per-provider counts / last activity |

## Optional: semantic search

`recall search` (FTS5) finds exact words. For "by meaning" matches, opt in to embeddings.

1. Get an API key (OpenAI: https://platform.openai.com/api-keys).
2. Export it: `export OPENAI_API_KEY=sk-...`
3. Create `~/.recall/config.toml`:

```toml
[embedding]
provider    = "openai"
model       = "text-embedding-3-small"
api_key_env = "OPENAI_API_KEY"
```

4. Run once after each `scan`:

```bash
recall embed       # embeds new/changed sessions
recall semantic "the one where I was wiring up oauth"
```

Embeddings are stored as raw `f32` vectors in the same SQLite DB. Cosine similarity is computed in memory — fine up to a few thousand sessions.

To use a different provider (Voyage / Cohere / local Ollama), extend the `embed_text` function in `src/main.rs` — it's ~10 lines.

## Data layout

```
~/.recall/
├── recall.db          # SQLite (sessions, sessions_fts, embeddings, edges)
└── config.toml        # OPTIONAL — only if you want embeddings
```

Schema (excerpt):

```sql
sessions       (id, provider, session_id, cwd, title, first/last_prompt, ...)
sessions_fts   FTS5 virtual table over title + prompts + body
embeddings     (session_pk → BLOB of f32 vector + model + dim)
edges          (src_pk, dst_pk, kind, weight)  -- 1-hop graph (same_cwd, future: shared_entity)
```

## Privacy & safety

- **Nothing leaves your machine** unless *you* configure an embedding API key. The `embed` step makes outbound HTTPS to whichever provider you chose — that's the only network call.
- **No telemetry.** Ever.
- `recall` only **reads** `~/.claude/projects/`, `~/.codex/history.jsonl`, and **writes** to `~/.recall/`. It does not touch your actual session files. `resume` execs the official `claude` / `codex` binary; recall itself never reads or mutates conversation state.

## FAQ

**Does it work without `claude` / `codex` installed?** Yes for `search` / `show`. `resume` execs the original CLI so it must be on `PATH` to actually relaunch.

**Multi-machine?** No, by design. recall is single-node. If you want central indexing across many machines, you're writing something different.

**Is it safe to run on stale jsonl?** Yes — `scan` is sha256-incremental, so it skips unchanged files.

**Will you add Cursor / Continue / Gemini / Aider?** Possibly, but only as upstream contributions. Single-tool focus keeps the picker honest.

## License

MIT. See [LICENSE](LICENSE).

## See also

- [README.ko.md](README.ko.md) — 한국어 안내
