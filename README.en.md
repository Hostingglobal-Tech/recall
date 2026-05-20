# recall

![recall banner](docs/banner.png)

> **One fuzzy phrase. Any past Claude Code / Codex session, back in front of you.**
> A skill that Claude Code / Codex invoke for you. 100% local, no API key, zero outbound traffic.

[한국어](README.md)

---

## The 30-second pitch

```
You: continue the deno auth-header session we had the other day.

AI:  ➜ recall search "deno auth header"
     2 matches. Most recent:
       claude :: 3cca0676-1106-4c5a-8f1f-1080ad72e4cd
     Paste this one-liner into the current CLI to return to that session:

        /resume 3cca0676-1106-4c5a-8f1f-1080ad72e4cd

You: [copy → paste → Enter]

[that session is alive again, inside the current CLI]
```

— You don't memorize `recall`'s commands. **The AI reads SKILL.md and dispatches on its own**, finds the session_id, and hands you a single `/resume <id>` line. You paste it once.

---

## Why

| Pain | recall |
|---|---|
| Hundreds of sessions — `claude --resume` / `codex resume` pickers can't surface the right one | Full-text match across body & prompts (SQLite FTS5) |
| You never renamed or forked — and now can't find the session | Just a keyword, no session_id needed |
| Cloud sync is overkill / unwanted | 100% local. Zero network calls, zero telemetry, zero API keys |
| Two tools, two scattered histories | Single index for Claude Code + Codex |
| Manual reindexing is annoying | `recall daemon install` -> auto-scan every 30 min |
| Spawning a new shell breaks your flow | Outputs the `/resume <id>` slash command — you paste it into the CLI you're already in |

---

## Install (recommended: let an AI do it)

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
codex "Install https://github.com/Hostingglobal-Tech/recall. If Rust is missing, install rustup. Clone to ~/.local/share/recall, cargo build --release, put binary on PATH, copy plugins/codex/SKILL.md to ~/.codex/skills/recall/SKILL.md, then run 'recall init && recall scan && recall daemon install'."
```

> If you use Claude Code and Codex on the same machine, register both skills:
> `cp plugins/claude/SKILL.md ~/.claude/skills/recall/SKILL.md` (Claude)
> `cp plugins/codex/SKILL.md ~/.codex/skills/recall/SKILL.md` (Codex)

After install, never type the commands again — just say what you remember:

> "continue yesterday's oauth wiring session"
> "where was the supabase RLS thing we did last week?"
> "resume the k8s ingress debugging from Tuesday"

---

## Install (manual, Rust 1.74+)

```bash
git clone https://github.com/Hostingglobal-Tech/recall.git ~/.local/share/recall
cd ~/.local/share/recall
cargo build --release
cp target/release/recall ~/.local/bin/

# Register the Claude Code skill
mkdir -p ~/.claude/skills/recall
cp plugins/claude/SKILL.md ~/.claude/skills/recall/SKILL.md

# Register the Codex skill (if you use Codex on this machine too)
mkdir -p ~/.codex/skills/recall
cp plugins/codex/SKILL.md ~/.codex/skills/recall/SKILL.md

# First index + auto-scan
recall init
recall scan
recall daemon install              # every 30 min
```

---

## resume — how it works

recall spawns nothing. It finds the session_id and prints the one-liner you should paste into your current CLI.

```bash
$ recall resume "deno auth header"
matched   : claude :: 3cca0676-1106-4c5a-8f1f-1080ad72e4cd
cwd       : /home/you/projects/foo

To resume this session, paste the following one-liner into your current CLI:

    /resume 3cca0676-1106-4c5a-8f1f-1080ad72e4cd
```

Both claude and codex accept `/resume <session_id>` as an in-session slash command. Paste it and the current CLI swaps in that session.

> If you'd rather spawn a fresh process, run `claude --resume <id>` or `codex resume <id>` in a new shell.

---

## Auto-indexing

The `recall daemon` keeps the DB in sync — you don't touch a thing.

```bash
recall daemon install --interval-min 30   # register (default 30 min)
recall daemon status                       # check
recall daemon uninstall                    # remove
```

Backends:
- Linux / macOS: adds a `crontab` line
- Windows: registers Scheduled Task `recall-scan`

---

## Commands the AI invokes (for reference)

You normally don't need to type these.

| Command | What it does |
|---|---|
| `recall init` | create `~/.recall/recall.db` |
| `recall scan [--provider claude\|codex\|all] [--force]` | sha256-incremental indexing |
| `recall search "<keyword>"` | FTS5 full-text |
| `recall show <session_id_prefix>` | metadata + first/last prompt |
| `recall resume <id\|keyword>` | locate session_id and print the `/resume <uuid>` one-liner (no spawn) |
| `recall related <session_id_prefix>` | sessions sharing the same `cwd` (1-hop graph) |
| `recall stats` | per-provider counts |
| `recall daemon install/status/uninstall` | manage auto-scan |

---

## Data layout

```
~/.recall/recall.db   # SQLite (sessions + FTS5 + edges)
```

Schema:
```sql
sessions       (id, provider, session_id, cwd, title, first/last_prompt, ...)
sessions_fts   FTS5 virtual table (title + prompts + body)
edges          (src_pk, dst_pk, kind, weight)   -- 1-hop graph (same_cwd)
```

No API key. No config file. No external services.

---

## Privacy

- Zero network calls. Install, build, run — nothing reaches out.
- No telemetry.
- Original session files are read-only. recall writes only to `~/.recall/`.
- recall spawns no other process. You paste `/resume` into your current CLI and it swaps the session in place.

---

## FAQ

### Does recall work without claude / codex installed?

The lookup commands do. `recall search`, `recall show`, `recall stats`, and `recall related` are read-only against recall's own SQLite DB, so they don't care whether the upstream CLIs are present. Indexing also works as long as `~/.claude/projects/` or `~/.codex/sessions/` directories already exist on disk from previous sessions.

The `/resume <uuid>` one-liner that `recall resume` prints is **a slash command that's only meaningful inside a running claude or codex CLI**. You need that CLI installed and active to paste the line and have it replace your current session. If you only want to search and archive past work, recall is fully usable on its own.

### Can I share recall across multiple machines?

No — single-node is deliberate. Three reasons:

1. **It would break the core promise.** recall's headline is "100% local, no API key, zero outbound traffic." Any form of cross-machine sync requires either a cloud or a sync server, which kills that promise.
2. **Session bodies routinely contain credentials.** Tokens, password fragments, internal API URLs leak into chat naturally. A sync channel is itself a new attack surface.
3. **`/resume` is single-host by design.** A session_id is only useful on the machine where claude or codex can find the corresponding jsonl on disk. A session_id from another machine won't open anywhere.

If you really want cross-machine search, two workarounds:
- Install recall separately on each machine and search per-machine.
- Put `~/.recall/recall.db` on shared storage (NAS, SMB) and mount it read-only elsewhere. Concurrent writes will deadlock on SQLite's WAL.

### Will old jsonl files be re-scanned every time? Doesn't that get slow?

scans are incremental. recall stores a **SHA256 of each jsonl file's full contents** in the DB; on the next scan it recomputes and skips any file whose hash still matches.

- Unchanged file: skip (no DB I/O).
- File with new messages appended: that one session is re-upserted; other sessions are unaffected.
- Brand-new file: fresh INSERT.

On a real machine with ~19 sessions / 7,900 messages / 48MB of raw data, a from-scratch scan finishes in about a second; an incremental scan is faster. The 30-minute daemon mostly produces all-skipped scans, so it costs nothing in steady state.

Need to force a full re-index (e.g. after changing parser logic)? `recall scan --force` bypasses the SHA check. To wipe the DB entirely, delete `~/.recall/recall.db` and run `recall init && recall scan`.

### Will you support Cursor / Continue / Gemini CLI / Aider too?

Two providers are officially supported today:

- **Claude Code**: `~/.claude/projects/<encoded-cwd>/<session-id>.jsonl`
- **Codex**: modern `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl` + legacy `~/.codex/history.jsonl` fallback

Most other tools use similar jsonl or SQLite-backed session stores, so adding a new provider is a localized change. The pattern:

1. Add a `<provider>_root() -> PathBuf` helper in `src/main.rs`.
2. Write `parse_<provider>(path) -> Result<SessionRow>` — only `session_id`, `cwd`, `first/last_ts`, and message bodies need to be extracted.
3. Wire a new branch into `cmd_scan()` (the claude branch is a clean template).

PRs welcome. Filing an issue like `provider: cursor` with the actual filesystem path used on your machine plus one anonymized sample file makes the addition much faster.

### Why isn't my search hitting? Does FTS5 handle Korean / non-ASCII?

recall's FTS5 uses `tokenize = 'unicode61 remove_diacritics 2'`, which tokenizes Korean (Hangul), Japanese, Chinese, Arabic, etc. as well as Latin scripts at the Unicode codepoint level. A few caveats:

- **No implicit prefix match.** `recall search "lock"` won't match "lockout"; supply the full token, or use the FTS5 wildcard form `lock*` (note that the keyword is shell-escaped, so test carefully).
- **Phrase search.** Quote multi-word terms — `recall search '"deno auth"'` — to require that they appear together.
- **Very common tokens drown the result.** Words like "the", "is", or particle-like fragments are not stop-listed; they match nearly every session. Reach for a unique noun (a library, an error code, a personal project name) and you'll narrow the result fast.

If a search returns nothing, vary the keyword: switch language (`lockout` ↔ `잠금`), break a compound up, or try a more distinctive token like a filename, error message, or library name.

### Can I change the DB location?

Yes. The global `--db <PATH>` flag is honored by every subcommand. Default is `~/.recall/recall.db`. Use a custom path for sandboxed testing (`--db /tmp/test.db`), to keep recall data off your backup target, or to put the DB on faster storage. Just pass the same `--db` to `daemon install` so the cron entry uses the same DB.

### How do I confirm the daemon is actually running?

```bash
recall daemon status
```

shows the OS scheduler entry (next run, registered command). Beyond that:

- **Linux / macOS**: `crontab -l | grep recall` to inspect the cron line; `tail -f /tmp/recall-scan.log` to watch output.
- **Windows**: `schtasks /query /tn recall-scan`, or open Task Scheduler and check "Last Run Result."

If you just finished a session and a search isn't hitting it yet, either wait up to 30 minutes for the daemon's next fire or run `recall scan` once manually.

### Cursor / IDE integration — can the resume happen automatically?

By design, no. recall never spawns another process; the `/resume <uuid>` line is only effective when the user pastes it into a running CLI (slash commands accept in-session input only). This separation buys you:

- No risk of recall launching an unintended shell or sandbox.
- Credentials, environment variables, and sandbox settings stay in the context you chose.
- You see the resume line and get one more glance before swapping sessions.

If you really need automation (e.g. a daily cron that re-opens yesterday's session), call `claude --resume <id>` or `codex resume <id>` directly — that's outside recall's scope, but you can feed it the `<id>` recall prints.

### How short can a session_id prefix be?

`recall show`, `recall resume`, and `recall related` all match with `SELECT ... WHERE session_id LIKE 'prefix%'`. With UUID v4 IDs, the first 8 characters are almost always unique (`3cca0676`, `019e2314`, …). If multiple sessions share a prefix, the most recent (`ORDER BY last_ts DESC`) wins.

The `SID_8` column that `recall search` prints **is** that 8-character prefix, so you can copy it straight from search output into `recall show 3cca0676`.

---

## License

[MIT](LICENSE)
