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
codex "Install https://github.com/Hostingglobal-Tech/recall. If Rust is missing, install rustup. Clone to ~/.local/share/recall, cargo build --release, put binary on PATH, copy plugins/codex/SKILL.md to ~/.codex/skills/recall/SKILL.md, then run 'recall init && recall scan && recall daemon install'."
```

> Claude Code 와 Codex 를 같은 머신에서 모두 쓴다면 두 skill 모두 등록하세요:
> `cp plugins/claude/SKILL.md ~/.claude/skills/recall/SKILL.md` (Claude)
> `cp plugins/codex/SKILL.md ~/.codex/skills/recall/SKILL.md` (Codex)

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

# Codex skill 등록 (Codex 도 쓰는 머신이면 같이)
mkdir -p ~/.codex/skills/recall
cp plugins/codex/SKILL.md ~/.codex/skills/recall/SKILL.md

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

### claude / codex 가 설치되어 있지 않아도 recall 을 쓸 수 있나?

조회 기능은 전부 됩니다. `recall search`, `recall show`, `recall stats`, `recall related` 는 recall 이 자기 SQLite DB 만 보면 되는 read-only 작업이라 claude / codex 본체 유무와 무관하게 작동합니다. 과거에 만들어진 `~/.claude/projects/` 또는 `~/.codex/sessions/` 디렉터리만 있으면 인덱싱도 됩니다.

`recall resume` 이 출력하는 `/resume <uuid>` 한 줄은 **claude 또는 codex CLI 안에서 입력해야 의미가 있는 slash command** 입니다. 그 CLI 가 설치되어 있고 실행 중이어야 그 한 줄이 현재 세션을 교체합니다. 단순히 어떤 작업이 있었는지 검색·아카이브 용도라면 claude / codex 가 없어도 unrestricted 사용 가능합니다.

### 여러 머신의 세션을 한 곳에서 검색할 수 있나?

의도적으로 single-node 전용으로 설계했습니다. 이유 세 가지:

1. **약속을 지키기 위해** — recall 의 핵심 약속은 "100% 로컬, API key 없음, 외부 전송 0" 입니다. 멀티-노드 동기화는 어떤 형태든 클라우드 또는 동기화 서버가 필요해서 그 약속이 깨집니다.
2. **세션 본문에 자격증명이 자주 섞임** — 토큰, 비밀번호 fragment, 사내 API URL 같은 게 자연스럽게 들어갑니다. 동기화 채널은 그 자체로 새 공격면입니다.
3. **`/resume` 의 작동 모델 자체가 단일 노드** — 받은 session_id 는 그 머신의 claude / codex 가 자기 디스크에서 해당 jsonl 을 찾을 때만 의미가 있습니다. 다른 머신의 session_id 를 받아도 거기서는 못 엽니다.

여러 머신을 굳이 통합하고 싶다면 두 가지 우회:
- 각 머신에 recall 을 따로 깔고 그 머신 안에서만 검색
- NAS 등 공유 스토리지에 `~/.recall/recall.db` 를 두고 read-only mount — 단, 동시 쓰기는 SQLite WAL 락으로 충돌 가능

### 오래된 세션 파일도 재 scan 되나? 매번 시간이 오래 걸리지 않나?

incremental scan 입니다. recall 은 각 jsonl 파일의 **전체 내용에 대한 SHA256 해시**를 DB 에 저장해두고, 다음 scan 때 다시 계산해 일치하면 그 파일은 건너뜁니다 (`skipped`).

- 변경 없는 파일: skip (DB I/O 0)
- 메시지가 더 추가된 파일: 그 세션만 다시 upsert (다른 세션 영향 X)
- 완전히 새로운 파일: 신규 INSERT

본 머신 기준 약 19 세션 / 7,900 메시지 / 48MB 의 raw 데이터를 fresh scan 했을 때 약 1초, incremental scan 은 그보다 더 빠릅니다. 30분 주기 daemon 이 돌아도 평소엔 거의 모든 파일이 skipped 라 부담 없습니다.

강제 전체 재 인덱싱이 필요한 경우 (예: 파서 로직이 바뀐 직후) `recall scan --force` 를 한 번 실행하면 sha 비교를 건너뜁니다. DB 자체를 초기화하려면 `~/.recall/recall.db` 삭제 후 `recall init && recall scan`.

### Cursor / Continue / Gemini CLI / Aider 같은 다른 AI 코딩 도구도 지원하나?

현재 공식 지원은 두 가지입니다:

- **Claude Code**: `~/.claude/projects/<encoded-cwd>/<session-id>.jsonl`
- **Codex**: 모던 `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl` + 레거시 `~/.codex/history.jsonl`

다른 도구들도 비슷한 jsonl 또는 SQLite 기반 세션 저장소를 가지고 있어서 어댑터 1개만 추가하면 동일 인덱스에 통합됩니다. 패턴:

1. `src/main.rs` 에 `<provider>_root() -> PathBuf` helper 추가
2. `parse_<provider>(path) -> Result<SessionRow>` 작성 — `session_id`, `cwd`, `first/last_ts`, 메시지 본문만 추출하면 됨
3. `cmd_scan()` 안에 새 provider branch 추가 (claude 측 코드를 그대로 모델로)

PR 환영합니다. 이슈에 `provider: cursor` 같은 식으로 적어주시고, 본인 머신의 실제 경로 + 파일 1개 샘플을 첨부하시면 추가가 훨씬 빠릅니다.

### 검색이 잘 안 잡힌다 / 한글이 토큰화 이상하다?

recall 의 FTS5 는 `tokenize = 'unicode61 remove_diacritics 2'` 로 설정되어 있어서 한글·영문 모두 유니코드 코드포인트 단위로 토큰화됩니다. 다만:

- **prefix 검색 안 됨** — `recall search "잠"` 으로 "잠금" 매칭은 안 됩니다. 단어 전체를 입력하거나 `잠*` 처럼 FTS5 prefix wildcard 를 직접 쓸 수 있습니다 (단, 키워드 인자에 escape 됨에 주의).
- **연어/구문 검색** — 띄어쓰기로 구분된 두 단어를 모두 포함하는 세션을 찾고 싶으면 `"잠금 정책"` 처럼 따옴표 안에 같이 넣으면 됩니다.
- **너무 흔한 단어는 노이즈** — "the", "is", "그리고" 같은 stop word 는 별도 제거 안 되어 있어서 거의 모든 세션에 매칭됩니다. 좀 더 고유한 키워드 (`wakelock`, `한진체`, `프나펑` 같은) 가 잘 좁힙니다.

조회 결과가 0건이면 키워드를 변형해보세요: 영문 ↔ 한글 (`lockout` ↔ `잠금`), 단어 분해 (`프나펑` → `FNF` 또는 `psych engine`), 또는 더 특이한 명사 (파일 이름·에러 메시지·라이브러리 이름).

### `--db` 옵션으로 DB 위치를 바꿀 수 있나?

네. `--db <PATH>` 전역 옵션이 모든 서브커맨드에 있습니다. 기본값은 `~/.recall/recall.db` 인데, 테스트용 격리 DB 를 따로 둘 수 있고 (`--db /tmp/test.db`), Time Machine / 백업에서 제외하고 싶다면 다른 경로로 옮길 수도 있습니다. daemon 도 동일 DB 를 가리키도록 install 시 `--db` 를 지정하면 됩니다.

### daemon 이 정말 도는지 어떻게 확인하나?

```bash
recall daemon status
```

가 OS 스케줄러에 등록된 정보 (다음 실행 시간, 등록 상태, 명령) 를 보여줍니다. 추가로:

- Linux / macOS: `crontab -l | grep recall` 로 cron 라인 직접 확인 / `tail -f /tmp/recall-scan.log` 로 출력 추적
- Windows: `schtasks /query /tn recall-scan` / Task Scheduler GUI 에서 "최근 실행" 확인

세션을 막 끝낸 직후 검색이 안 잡힌다면 daemon 의 다음 fire 까지 (최대 30분) 기다리거나 `recall scan` 을 직접 한 번 호출하면 됩니다.

### Cursor / IDE 와 통합 — 자동으로 띄울 수 있나?

recall 은 일부러 다른 프로세스를 spawn 하지 않습니다. `/resume <uuid>` 한 줄은 사용자가 직접 자기 CLI 에 입력해야 의미가 있습니다 (slash command 가 in-session 입력만 인식). 이 분리 덕분에:

- recall 이 의도하지 않은 새 셸/프로세스를 띄울 위험 0
- 자격증명·환경변수·sandbox 설정이 의도와 다른 컨텍스트로 새지 않음
- 사용자가 `/resume` 줄을 보고 한 번 더 확인할 기회가 있음

자동화가 꼭 필요한 경우 (예: 일일 cron 으로 어제 세션 재 open) 는 `claude --resume <id>` 또는 `codex resume <id>` 를 새 셸에서 직접 호출하시면 됩니다 — 그건 recall 의 범위 밖이지만 출력된 `<id>` 를 그대로 쓸 수 있습니다.

### session_id prefix 는 몇 글자까지 줄여 써도 되나?

`recall show` / `recall resume` / `recall related` 모두 `SELECT ... WHERE session_id LIKE 'prefix%'` 로 매칭합니다. UUID v4 기준 앞 8 자리면 보통 충돌 없이 유일하게 잡힙니다 (`3cca0676`, `019e2314` 같은 형태). 동일 prefix 가 여러 세션에 걸치면 `last_ts DESC` 순으로 가장 최근 것이 선택됩니다.

검색 결과 (`recall search`) 가 보여주는 `SID_8` 컬럼이 바로 그 8자 prefix 라서 그대로 복사해 `recall show 3cca0676` 식으로 쓰면 됩니다.

---

## 라이선스

[MIT](LICENSE)
