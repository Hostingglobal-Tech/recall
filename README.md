# recall

![recall banner](docs/banner.png)

> **흐릿한 한 마디로 과거 Claude Code / Codex 세션을 되찾는다.**
> Claude Code / Codex 안에서 AI 가 자동 호출하는 skill. 100% 로컬, API key 없음, 외부 전송 0.

[English](README.en.md)

---

## ⚡ 30초 핵심

```
You: 그때 deno 인증 헤더 작업했던 거 다시 시작해줘.

AI:  ➜ recall search "deno 인증 헤더"
     2건 매칭. 최근 claude 세션 resume 합니다.
     ➜ recall resume 3cca0676

[그 세션이 원래 cwd 에서 그대로 살아남]
```

— `recall` 명령을 외울 필요 없습니다. **AI 가 SKILL.md 를 읽고 알아서 호출**합니다.

---

## 🧩 왜

| 문제 | recall |
|---|---|
| 세션이 수백 개 쌓이면 `claude --resume` / `codex resume` 의 picker 가 못 찾는다 | 흐릿한 한 마디로 본문까지 풀텍스트 매칭 (SQLite FTS5) |
| rename · fork 없이 작업하면 그 세션을 다시 못 찾는다 | session_id 모르고 키워드만 있어도 OK |
| 클라우드 동기화는 부담스럽다 | **100% 로컬**. 네트워크 호출 0, 텔레메트리 0, API key 0 |
| Claude / Codex 따로 검색은 귀찮다 | 한 인덱스에 두 도구 통합 |
| 매번 수동 인덱싱은 불편하다 | `recall daemon install` 한 번이면 30분마다 자동 |

---

## 🚀 설치 (권장: AI 에게 한 줄 던지기)

이미 Claude Code 또는 Codex 가 OAuth 인증된 상태라면, 다음을 그 도구에 그대로 붙여넣으세요.

### Claude Code

```
https://github.com/Hostingglobal-Tech/recall 를 이 머신에 설치해줘.

1. Rust 없으면 rustup 으로 설치
2. ~/.local/share/recall 에 clone → cargo build --release
3. 빌드 산출물(target/release/recall)을 PATH 디렉토리(~/.local/bin 등)에 복사
4. plugins/claude/SKILL.md 를 ~/.claude/skills/recall/SKILL.md 로 복사 (skill 등록)
5. recall init && recall scan && recall daemon install
6. 단계마다 결과 확인
```

### Codex

```bash
codex "Install https://github.com/Hostingglobal-Tech/recall. If Rust is missing, install rustup. Clone to ~/.local/share/recall, cargo build --release, put binary on PATH, copy plugins/claude/SKILL.md to ~/.claude/skills/recall/SKILL.md, then run 'recall init && recall scan && recall daemon install'."
```

설치 후 — **명령을 외우지 마세요.** 그냥 자연어로 말하면 됩니다:

> "그때 oauth 연결하던 작업 다시"
> "지난주 supabase RLS 짰던 세션 어디였지?"
> "어제 K8s 인그레스 디버깅 그거 이어서"

---

## 🛠️ 설치 (수동, Rust 1.74+)

```bash
git clone https://github.com/Hostingglobal-Tech/recall.git ~/.local/share/recall
cd ~/.local/share/recall
cargo build --release
cp target/release/recall ~/.local/bin/

# Claude Code skill 등록
mkdir -p ~/.claude/skills/recall
cp plugins/claude/SKILL.md ~/.claude/skills/recall/SKILL.md

# 첫 인덱싱 + 자동 인덱싱 등록
recall init
recall scan
recall daemon install            # 30분 주기 자동 scan
```

---

## 🔁 자동 인덱싱

새 세션은 `recall daemon` 이 알아서 DB 에 넣어줍니다. 사람은 손 안 댑니다.

```bash
recall daemon install --interval-min 30   # 등록 (기본 30분)
recall daemon status                       # 등록 상태 확인
recall daemon uninstall                    # 제거
```

OS 별 백엔드:
- **Linux / macOS** → `crontab` 한 줄 추가
- **Windows** → Scheduled Task `recall-scan` 등록

---

## 🤖 AI 가 부르는 명령 (참고)

사람이 외울 필요 없음. SKILL.md 안내대로 AI 가 호출.

| 명령 | 동작 |
|---|---|
| `recall init` | `~/.recall/recall.db` 생성 |
| `recall scan [--provider claude\|codex\|all] [--force]` | sha256 증분 인덱싱 |
| `recall search "<키워드>"` | FTS5 풀텍스트 (title + first/last prompt + body) |
| `recall show <session_id_prefix>` | 세션 상세 + first/last prompt |
| `recall resume <id\|키워드> [--dry-run]` | `claude --resume` / `codex resume` 원래 cwd 에서 실행 |
| `recall related <session_id_prefix>` | 같은 cwd 의 다른 세션 (1-hop 그래프) |
| `recall stats` | provider 별 세션·메시지·사이즈 통계 |
| `recall daemon install/status/uninstall` | 주기 자동 scan 관리 |

---

## 📂 데이터 위치

```
~/.recall/recall.db   # SQLite (sessions + FTS5 + edges 만)
```

스키마:
```sql
sessions       (id, provider, session_id, cwd, title, first/last_prompt, ...)
sessions_fts   FTS5 가상 테이블 (title + prompts + body)
edges          (src_pk, dst_pk, kind, weight)   -- 1-hop 그래프 (same_cwd)
```

API key 도, config 파일도, 외부 의존도 **없습니다**.

---

## 🔒 프라이버시

- **네트워크 호출 0.** 설치·빌드·실행 어디에도 외부 통신 없음.
- **텔레메트리 0.**
- 원본 세션 파일은 **읽기만**. `recall` 은 `~/.recall/` 에만 씁니다.
- `resume` 는 공식 `claude` / `codex` 바이너리를 exec — recall 이 대화 상태를 건드리지 않습니다.

---

## ❓ FAQ

**`claude` / `codex` 없어도 됨?** `search` / `show` 는 OK. `resume` 는 원본 CLI 필요.

**여러 머신 통합?** 의도적으로 X — 단일 노드 전용.

**오래된 jsonl 재 scan?** sha256 증분이라 변경 없는 파일은 skip.

**Cursor / Continue / Gemini / Aider 도?** PR 환영.

---

## 📜 라이선스

[MIT](LICENSE)
