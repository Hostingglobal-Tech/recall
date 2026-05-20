# recall

![recall banner](docs/banner.png)

> **흐릿한 한 마디로 과거 Claude Code / Codex 세션을 되찾는다.**
> Claude Code / Codex 안에서 AI 가 자동 호출하는 skill. 100% 로컬, API key 없음, 외부 전송 0.

[English](README.en.md)

---

## 30초 핵심

```
You: 그때 deno 인증 헤더 작업했던 거 다시 시작해줘.

AI:  ➜ recall search "deno 인증 헤더"
     2건 매칭. 가장 최근 세션:
       claude :: 3cca0676-1106-4c5a-8f1f-1080ad72e4cd
     아래 한 줄을 그대로 입력하시면 그 세션으로 돌아갑니다:

        /resume 3cca0676-1106-4c5a-8f1f-1080ad72e4cd

You: [복사 → 붙여넣기 → Enter]

[그 세션이 현재 CLI 안에서 그대로 살아남]
```

— `recall` 명령을 외울 필요 없습니다. **AI 가 SKILL.md 를 읽고 알아서 호출**, session_id 를 찾아주고 사용자에게 `/resume <id>` 한 줄만 안내합니다. 사용자는 한 번 붙여넣기만 하면 됩니다.

---

## 왜

| 문제 | recall |
|---|---|
| 세션이 수백 개 쌓이면 `claude --resume` / `codex resume` 의 picker 가 못 찾는다 | 흐릿한 한 마디로 본문까지 풀텍스트 매칭 (SQLite FTS5) |
| rename · fork 없이 작업하면 그 세션을 다시 못 찾는다 | session_id 모르고 키워드만 있어도 OK |
| 클라우드 동기화는 부담스럽다 | 100% 로컬. 네트워크 호출 0, 텔레메트리 0, API key 0 |
| Claude / Codex 따로 검색은 귀찮다 | 한 인덱스에 두 도구 통합 |
| 매번 수동 인덱싱은 불편하다 | `recall daemon install` 한 번이면 30분마다 자동 |
| 외부 새 셸 띄우면 작업 흐름이 끊긴다 | `/resume <id>` slash command 한 줄로 현재 CLI 안에서 그 세션으로 점프 |

---

## 설치 (권장: AI 에게 한 줄 던지기)

이미 Claude Code 또는 Codex 가 OAuth 인증된 상태라면, 다음을 그 도구에 그대로 붙여넣으세요.

### Claude Code

```
https://github.com/Hostingglobal-Tech/recall 를 이 머신에 설치해줘.

1. Rust 없으면 rustup 으로 설치
2. ~/.local/share/recall 에 clone, cargo build --release
3. 빌드 산출물(target/release/recall)을 PATH 디렉토리(~/.local/bin 등)에 복사
4. plugins/claude/SKILL.md 를 ~/.claude/skills/recall/SKILL.md 로 복사 (skill 등록)
5. recall init && recall scan && recall daemon install
6. 단계마다 결과 확인
```

### Codex

```bash
codex "Install https://github.com/Hostingglobal-Tech/recall. If Rust is missing, install rustup. Clone to ~/.local/share/recall, cargo build --release, put binary on PATH, copy plugins/claude/SKILL.md to ~/.claude/skills/recall/SKILL.md, then run 'recall init && recall scan && recall daemon install'."
```

설치 후 — 명령을 외우지 마세요. 그냥 자연어로 말하면 됩니다:

> "그때 oauth 연결하던 작업 다시"
> "지난주 supabase RLS 짰던 세션 어디였지?"
> "어제 K8s 인그레스 디버깅 그거 이어서"

---

## 설치 (수동, Rust 1.74+)

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

## resume — 어떻게 동작하나

recall 은 어떤 프로세스도 spawn 하지 않습니다. session_id 를 찾아주고 사용자가 현재 CLI 에 직접 붙여넣을 한 줄만 안내합니다.

```bash
$ recall resume "deno 인증 헤더"
matched   : claude :: 3cca0676-1106-4c5a-8f1f-1080ad72e4cd
cwd       : C:\Users\ceo

To resume this session, paste the following one-liner into your current CLI:

    /resume 3cca0676-1106-4c5a-8f1f-1080ad72e4cd
```

claude 와 codex 둘 다 `/resume <session_id>` 를 in-session slash command 로 지원합니다. 사용자가 그 한 줄을 그대로 입력하면 현재 CLI 가 그 세션으로 교체됩니다.

> 새 프로세스로 외부 실행을 원하면 `claude --resume <id>` 또는 `codex resume <id>` 를 새 셸에서 실행하면 됩니다.

---

## 자동 인덱싱

새 세션은 `recall daemon` 이 알아서 DB 에 넣어줍니다.

```bash
recall daemon install --interval-min 30   # 등록 (기본 30분)
recall daemon status                       # 상태 확인
recall daemon uninstall                    # 제거
```

OS 별 백엔드:
- Linux / macOS: `crontab` 한 줄 추가
- Windows: Scheduled Task `recall-scan` 등록

---

## AI 가 부르는 명령 (참고)

사람은 보통 외울 필요 없음. AI 가 SKILL.md 안내대로 호출.

| 명령 | 동작 |
|---|---|
| `recall init` | `~/.recall/recall.db` 생성 |
| `recall scan [--provider claude\|codex\|all] [--force]` | sha256 증분 인덱싱 |
| `recall search "<키워드>"` | FTS5 풀텍스트 (title + first/last prompt + body) |
| `recall show <session_id_prefix>` | 세션 상세 + first/last prompt |
| `recall resume <id\|키워드>` | session_id 찾아서 `/resume <uuid>` 한 줄만 안내 (실행 안 함) |
| `recall related <session_id_prefix>` | 같은 cwd 의 다른 세션 (1-hop 그래프) |
| `recall stats` | provider 별 세션·메시지·사이즈 통계 |
| `recall daemon install/status/uninstall` | 주기 자동 scan 관리 |

---

## 데이터 위치

```
~/.recall/recall.db   # SQLite (sessions + FTS5 + edges)
```

스키마:
```sql
sessions       (id, provider, session_id, cwd, title, first/last_prompt, ...)
sessions_fts   FTS5 가상 테이블 (title + prompts + body)
edges          (src_pk, dst_pk, kind, weight)   -- 1-hop 그래프 (same_cwd)
```

API key, config 파일, 외부 의존 모두 없습니다.

---

## 프라이버시

- 네트워크 호출 0. 설치·빌드·실행 어디에도 외부 통신 없음.
- 텔레메트리 0.
- 원본 세션 파일은 읽기만. recall 은 `~/.recall/` 에만 씁니다.
- recall 은 다른 프로세스를 spawn 하지 않습니다. 사용자가 `/resume` 슬래시 명령을 직접 입력해 현재 CLI 세션을 그 세션으로 교체합니다.

---

## FAQ

**claude / codex 없어도 됨?** `search` / `show` 는 OK. `resume` 안내대로 입력하려면 해당 CLI 가 실행 중이어야 합니다.

**여러 머신 통합?** 의도적으로 X — 단일 노드 전용.

**오래된 jsonl 재 scan?** sha256 증분이라 변경 없는 파일은 skip.

**Cursor / Continue / Gemini / Aider 도?** PR 환영.

---

## 라이선스

[MIT](LICENSE)
