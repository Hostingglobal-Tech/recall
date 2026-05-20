# recall

![recall banner](docs/banner.png)

> **One fuzzy phrase. Any past Claude Code / Codex session, back at your fingertips.**
> Not a CLI you type by hand — a **skill that Claude Code / Codex call for you, in-place, when you describe what you vaguely remember.**

[한국어 README](README.md)

---

## How you actually use it

You never have to rename or fork a session. Inside Claude Code or Codex, just say what you remember in plain words:

```
You: continue the deno auth-header session we had the other day.

AI:  Searching with recall.
     ➜ recall search "deno auth header"
     2 matches:
       • claude / 3cca0676 / "wire auth header to deno"   / 2026-05-20
       • codex  / 019e3891 / "...Deno WS auth header mid" / 2026-05-18
     Resuming the more recent claude session.
     ➜ recall resume 3cca0676

[that session is alive again, in the original cwd]
```

You don't have to remember `recall`'s commands. **The AI reads SKILL.md and dispatches on its own.**

---

## Why

`claude --resume` and `codex resume` both ship a picker, but once you have hundreds of sessions the picker doesn't tell you *what each one was trying to do.* You never renamed them. You never forked them. The only thing you remember is the topic. recall takes that topic and lands you back in the right session.

- **Local-first.** Sessions never leave the box. No cloud sync, no telemetry.
- **Two tools, one index.** Claude Code (`~/.claude/projects/`) + Codex (`~/.codex/history.jsonl`) co-located.
- **FTS5 full-text + optional embeddings.** Keyword first; bring your own API key for semantic ("by meaning") fallback.
- **One-click resume.** The AI dispatches to `claude --resume` / `codex resume` in the original cwd.
- **A skill for AI agents.** SKILL.md is keyed on Korean + English trigger phrases so natural-language requests just work.

---

## Install — let an AI do it (recommended)

If you already have Claude Code or Codex installed and OAuth-authenticated, hand the whole install off in one paste. Dependencies, build, PATH, **skill registration**, first index — all of it.

### With Claude Code

Inside `claude`:

```
Please install https://github.com/Hostingglobal-Tech/recall on this machine.

1. If Rust is missing, install rustup.
2. Clone the repo to ~/.local/share/recall and run cargo build --release.
3. Copy target/release/recall into a PATH directory (e.g. ~/.local/bin).
4. Copy plugins/claude/SKILL.md to ~/.claude/skills/recall/SKILL.md (skill registration).
5. Run `recall init && recall scan`.

Confirm each step as you go.
```

### With Codex

One shell line:

```bash
codex "Install https://github.com/Hostingglobal-Tech/recall on this machine.\
 1) install rustup if missing,\
 2) clone to ~/.local/share/recall and cargo build --release,\
 3) copy the binary into ~/.local/bin (or any PATH dir),\
 4) copy plugins/claude/SKILL.md to ~/.claude/skills/recall/SKILL.md so AI agents can use it as a skill,\
 5) finally run 'recall init && recall scan'.\
 Confirm each step."
```

Once installed, **you never type `recall` commands.** You just talk:

```
> continue yesterday's oauth wiring session.
> where did we hash out that supabase RLS policy last week?
> resume the k8s ingress debugging we did Tuesday.
```

---

## Install — manual (optional)

Rust 1.74+ required.

```bash
git clone https://github.com/Hostingglobal-Tech/recall.git ~/.local/share/recall
cd ~/.local/share/recall
cargo build --release
cp target/release/recall ~/.local/bin/

# register the skill so Claude Code picks it up
mkdir -p ~/.claude/skills/recall
cp plugins/claude/SKILL.md ~/.claude/skills/recall/SKILL.md

# first index
recall init
recall scan
```

---

## Commands the AI invokes (for reference)

You normally don't need to type these. The AI calls them per SKILL.md.

| Command | What it does |
|---|---|
| `recall init` | create `~/.recall/recall.db` (idempotent) |
| `recall scan [--provider claude\|codex\|all] [--force]` | walk local session files, upsert into DB |
| `recall search "<keyword>"` | FTS5 full-text search across title / prompts / body |
| `recall semantic "<keyword>"` | embedding cosine top-K (requires API key) |
| `recall show <session_id_prefix>` | show metadata + first/last prompt |
| `recall resume <id\|keyword>` | dispatch to `claude --resume` / `codex resume` in the original cwd |
| `recall related <session_id_prefix>` | sessions sharing the same `cwd` (1-hop graph) |
| `recall embed [--provider all] [--force]` | embed sessions (requires API key) |
| `recall stats` | per-provider counts / last activity |

---

## Optional: semantic search

`recall search` (FTS5) matches keywords. For "by meaning" matches, opt into embeddings:

1. Get an OpenAI API key (https://platform.openai.com/api-keys).
2. `export OPENAI_API_KEY=sk-...`
3. `~/.recall/config.toml`:
   ```toml
   [embedding]
   provider    = "openai"
   model       = "text-embedding-3-small"
   api_key_env = "OPENAI_API_KEY"
   ```
4. After each `scan`:
   ```bash
   recall embed
   ```

The AI will now use semantic fallback automatically (SKILL.md instructs it).

To use a different provider (Voyage / Cohere / local Ollama), extend `embed_text` in `src/main.rs` — ~10 lines.

---

## Data layout

```
~/.recall/
├── recall.db          # SQLite (sessions, sessions_fts, embeddings, edges)
└── config.toml        # OPTIONAL — only if you want embeddings
```

Schema:

```sql
sessions       (id, provider, session_id, cwd, title, first/last_prompt, ...)
sessions_fts   FTS5 virtual table over title + prompts + body
embeddings     (session_pk → BLOB of f32 vector + model + dim)
edges          (src_pk, dst_pk, kind, weight)  -- 1-hop graph (same_cwd, future: shared_entity)
```

---

## Privacy & safety

- **Nothing leaves your machine** unless *you* configure an embedding API key (HTTPS to your chosen provider — the only network call).
- **No telemetry.**
- recall **reads** `~/.claude/projects/`, `~/.codex/history.jsonl`, and **writes** only to `~/.recall/`. Original session files are untouched.
- `resume` execs the official `claude` / `codex` binary — recall never mutates conversation state.

---

## FAQ

**Does it work without `claude` / `codex` installed?** `search` / `show` yes. `resume` needs the original CLI on PATH.

**Multi-machine?** No — single-node by design.

**Re-scanning old jsonl?** sha256-incremental — unchanged files skip.

**Cursor / Continue / Gemini / Aider?** PRs welcome. Single-tool focus keeps the picker honest.

---

## License

MIT — [LICENSE](LICENSE)
