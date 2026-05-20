# recall

> **희미한 기억만으로 과거 Claude Code / Codex 세션을 찾아 그 작업을 그대로 이어가세요.**
> 로컬 전용. SQLite (FTS5) + 선택적 임베딩 벡터 검색. `claude --resume` / `codex resume` 한 줄 실행.

`claude --resume` 도 `codex resume` 도 세션 picker 를 띄워주지만, 세션이 수백 개 쌓이면 **그때 그 작업이 어디 세션이었는지** 도저히 못 찾습니다. rename 해두지 않았고, fork 도 안 해뒀습니다. 기억나는 건 **대략의 주제** 뿐. `recall` 은 정확히 그 순간을 위해 만들었습니다.

```
$ recall search "deno 인증 헤더"
PROV    SID_8      LAST_TS              TITLE                          EXCERPT
claude  3cca0676   2026-05-20 10:00     deno 웹소켓 인증 통합            …«인증» 토큰을 WebSocket «헤더»로…
codex   019e3891   2026-05-18 10:08                                    …Deno WS «인증 헤더» 작업 중…

$ recall resume "deno 인증 헤더"
[recall resume] claude :: 3cca0676-1106-4c5a-8f1f-1080ad72e4cd
# 원래 cwd 에서 `claude --resume 3cca0676-…` 자동 실행
```

## 왜

CLI 에이전트의 핵심은 **하던 자리에서 이어가는 것**. 그런데 "하던 자리" 는 본 적 없는 UUID 와 무엇을 하던 세션인지 알 수 없는 picker 로만 식별됩니다. `recall` 은 모든 세션을 로컬에 인덱싱하고, **사람이 기억하는 방식 — 흐릿한 키워드 — 으로 그 세션을 찾아줍니다.**

- **로컬 전용.** 세션은 절대 외부로 안 나갑니다. 클라우드 동기화 없음, 텔레메트리 없음.
- **두 도구 통합.** Claude Code (`~/.claude/projects/**/*.jsonl`) + Codex (`~/.codex/history.jsonl`) 한 곳에서.
- **FTS5 풀텍스트 + 선택적 벡터 검색.** SQLite FTS5 가 정확한 키워드 매칭, 본인 API key 로 임베딩 의미 검색 추가 가능.
- **원클릭 resume.** `recall resume <id|키워드>` → 적절한 CLI (`claude --resume` / `codex resume`) 를 원래 cwd 에서 자동 실행.

## 설치

### 소스 빌드 (Rust 1.74+)

```bash
git clone https://github.com/Hostingglobal-Tech/recall.git
cd recall
cargo build --release
# 빌드 결과: target/release/recall (Linux/macOS) / target\release\recall.exe (Windows)
cp target/release/recall ~/.local/bin/
```

### 첫 실행

```bash
recall init
recall scan
```

`init` 이 `~/.recall/recall.db` 를 만들고, `scan` 이 `~/.claude/projects/` 와 `~/.codex/history.jsonl` 을 순회해서 SQLite 에 인덱싱합니다.

## 명령

| 명령 | 동작 |
|---|---|
| `recall init` | `~/.recall/recall.db` 생성 (idempotent) |
| `recall scan [--provider claude\|codex\|all] [--force]` | 로컬 세션 파일 인덱싱. sha256 변경분만 upsert. |
| `recall search "<키워드>"` | title / first_prompt / last_prompt / body 풀텍스트 FTS5 검색 |
| `recall semantic "<키워드>"` | 임베딩 cosine top-K (API key 필요, 아래 참조) |
| `recall show <session_id_prefix>` | 세션 상세 (메타 + first/last prompt) |
| `recall resume <id\|키워드> [--dry-run]` | `claude --resume` 또는 `codex resume` 자동 분기 + 원래 cwd 실행 |
| `recall related <session_id_prefix>` | 같은 cwd 의 다른 세션 (1-hop 그래프) |
| `recall embed [--provider all] [--force]` | 임베딩 생성 (API key 필요) |
| `recall stats` | provider 별 세션/메시지/사이즈 통계 |

## 선택: 의미 검색 (semantic)

`recall search` (FTS5) 는 키워드 정확 매칭. "비슷한 의미" 까지 잡으려면 임베딩 활성화.

1. API key 발급 — OpenAI: https://platform.openai.com/api-keys
2. 환경변수: `export OPENAI_API_KEY=sk-...`
3. `~/.recall/config.toml` 생성:

```toml
[embedding]
provider    = "openai"
model       = "text-embedding-3-small"
api_key_env = "OPENAI_API_KEY"
```

4. `scan` 후 한 번씩 `embed`:

```bash
recall embed       # 새/변경된 세션만 임베딩
recall semantic "그때 oauth 연결하던 그 작업"
```

임베딩은 raw `f32` 벡터로 같은 SQLite DB 의 `embeddings` 테이블에 저장. cosine 유사도는 인메모리 계산 — 수천 세션까지 빠릅니다.

다른 provider (Voyage / Cohere / 로컬 Ollama) 쓰려면 `src/main.rs` 의 `embed_text` 함수에 분기 한 줄 추가 (10 줄 내외).

## 데이터 위치

```
~/.recall/
├── recall.db          # SQLite (sessions, sessions_fts, embeddings, edges)
└── config.toml        # 선택사항 — 임베딩 쓸 때만
```

스키마 요약:

```sql
sessions       (id, provider, session_id, cwd, title, first/last_prompt, ...)
sessions_fts   FTS5 가상 테이블 (title + prompts + body)
embeddings     (session_pk → f32 벡터 BLOB + model + dim)
edges          (src_pk, dst_pk, kind, weight)  -- 1-hop 그래프 (same_cwd, 향후 shared_entity 추가)
```

## 프라이버시 & 안전

- **외부 전송은 없습니다.** 본인이 임베딩 API key 를 설정한 경우에만 그 provider 로 HTTPS 호출이 나갑니다. 그게 유일한 네트워크 활동.
- **텔레메트리 없음.** 한 번도.
- recall 은 `~/.claude/projects/`, `~/.codex/history.jsonl` 을 **읽기만** 하고, `~/.recall/` 에만 **씁니다.** 원본 세션 파일은 건드리지 않습니다. `resume` 는 공식 `claude` / `codex` 바이너리를 그대로 exec — recall 자체는 세션 내용을 변경하지 않습니다.

## FAQ

**`claude` / `codex` 가 설치 안 돼있어도 됩니까?** `search` / `show` 는 됩니다. `resume` 는 원본 CLI 를 exec 하니까 PATH 에 있어야 실제 재실행됩니다.

**여러 머신 통합?** 의도적으로 안 합니다. recall 은 단일 노드 전용. 여러 머신 중앙 인덱싱이 필요하면 그건 다른 도구를 만드는 겁니다.

**오래된 jsonl 다시 scan 해도 됩니까?** 네 — `scan` 은 sha256 증분이라 변경 없는 파일은 skip.

**Cursor / Continue / Gemini / Aider 도 지원?** PR 환영. 다만 단일 도구 집중이 picker 의 정직함을 유지합니다.

## 라이선스

MIT. [LICENSE](LICENSE) 참조.
