# recall

![recall banner](docs/banner.png)

> **One fuzzy phrase. Any past Claude Code / Codex session, back in front of you.**
> A skill that Claude Code / Codex invoke for you. 100% local, no API key, zero outbound traffic.

[한국어](README.md)

---

## ⚡ The 30-second pitch

```
You: continue the deno auth-header session we had the other day.

AI:  ➜ recall search "deno auth header"
     2 matches. Resuming the more recent claude session.
     ➜ recall resume 3cca0676

[that session is alive again, in the original cwd]
```

— You don't memorize `recall`'s commands. **The AI reads SKILL.md and dispatches on its own.**

---

## 🧩 Why

| Pain | recall |
|---|---|
| Hundreds of sessions — `claude --resume` / `codex resume` pickers can't surface the right one | Full-text match across body & prompts (SQLite FTS5) |
| You never renamed or forked — and now can't find the session | Just a keyword, no session_id needed |
| Cloud sync is overkill / unwanted | **100% local**. Zero network calls, zero telemetry, zero API keys |
| Two tools, two scattered histories | Single index for Claude Code + Codex |
| Manual reindexing is annoying | `recall daemon install` → auto-scan every 30 min |

---

## 🚀 Install — let an AI do it (recommended)

If Claude Code or Codex is already OAuth-authenticated, paste this in and walk away.

### Claude Code

```
Please install https://github.com/Hostingglobal-Tech/recall on this machine.

1. If Rust is missing, install rustup.
2. Clone to ~/.local/share/recall and run cargo build --release.
3. Copy target/release/recall to a PATH dir (e.g. ~/.local/bin).
4. Copy plugins/claude/SKILL.md to ~/.claude/skills/recall/SKILL.md (skill registration).
5. Run recall init && recall scan && recall daemon install.
6. Confirm each step.
```

### Codex

```bash
codex "Install https://github.com/Hostingglobal-Tech/recall. If Rust is missing, install rustup. Clone to ~/.local/share/recall, cargo build --release, put binary on PATH, copy plugins/claude/SKILL.md to ~/.claude/skills/recall/SKILL.md, then run 'recall init && recall scan && recall daemon install'."
```

After install, **never type the commands again** — just say what you remember:

> "continue yesterday's oauth wiring session"
> "where was the supabase RLS thing we did last week?"
> "resume the k8s ingress debugging from Tuesday"

---

## 🛠️ Install — manual (Rust 1.74+)

```bash
git clone https://github.com/Hostingglobal-Tech/recall.git ~/.local/share/recall
cd ~/.local/share/recall
cargo build --release
cp target/release/recall ~/.local/bin/

# Register the skill
mkdir -p ~/.claude/skills/recall
cp plugins/claude/SKILL.md ~/.claude/skills/recall/SKILL.md

# First index + auto-scan
recall init
recall scan
recall daemon install              # every 30 min
```

---

## 🔁 Auto-indexing

New sessions get picked up automatically — you don't touch a thing.

```bash
recall daemon install --interval-min 30   # register (default 30 min)
recall daemon status                       # check registration
recall daemon uninstall                    # remove
```

Backends:
- **Linux / macOS** → adds a `crontab` line
- **Windows** → registers Scheduled Task `recall-scan`

---

## 🤖 Commands the AI invokes (for reference)

You normally don't need to type these.

| Command | What it does |
|---|---|
| `recall init` | create `~/.recall/recall.db` |
| `recall scan [--provider claude\|codex\|all] [--force]` | sha256-incremental indexing |
| `recall search "<keyword>"` | FTS5 full-text (title + first/last prompt + body) |
| `recall show <session_id_prefix>` | metadata + first/last prompt |
| `recall resume <id\|keyword> [--dry-run]` | dispatch `claude --resume` / `codex resume` in original cwd |
| `recall related <session_id_prefix>` | sessions sharing the same `cwd` (1-hop graph) |
| `recall stats` | per-provider counts |
| `recall daemon install/status/uninstall` | manage auto-scan |

---

## 📂 Data layout

```
~/.recall/recall.db   # SQLite (sessions + FTS5 + edges only)
```

Schema:
```sql
sessions       (id, provider, session_id, cwd, title, first/last_prompt, ...)
sessions_fts   FTS5 virtual table (title + prompts + body)
edges          (src_pk, dst_pk, kind, weight)   -- 1-hop graph (same_cwd)
```

No API key. No config file. No external services.

---

## 🔒 Privacy

- **Zero network calls.** Install, build, run — nothing reaches out.
- **No telemetry.**
- Original session files are **read-only**. `recall` writes only to `~/.recall/`.
- `resume` execs the official `claude` / `codex` binary — recall never mutates conversation state.

---

## ❓ FAQ

**Does it work without `claude` / `codex` installed?** `search` / `show` yes. `resume` needs the original CLI on PATH.

**Multi-machine?** No — single-node by design.

**Re-scanning old jsonl?** sha256-incremental — unchanged files skip.

**Cursor / Continue / Gemini / Aider?** PRs welcome.

---

## 📜 License

[MIT](LICENSE)
