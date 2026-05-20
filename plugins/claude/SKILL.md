---
name: recall
description: 사용자가 과거 Claude Code 또는 Codex 세션을 흐릿한 기억으로 다시 찾고 그 자리에서 작업을 이어가려 할 때 호출. "그때 했던 작업 다시", "이전 세션 어디였지", "X 작업 했던 세션 찾아", "session resume", "fork 안 해놨는데 그거 다시", "어제 한 그거 이어서" 같은 표현 감지 시 사용. CLI 도구 `recall` 이 시스템에 설치되어 있어야 함 (없으면 사용자 권한 받고 README 의 설치 가이드 실행).
---

# recall — 과거 세션 재발견 + resume

사용자는 일반적으로 session_id 를 기억하지 못합니다. 흐릿한 주제 키워드만 알려줍니다. 너는 `recall` CLI 의 FTS5 풀텍스트 검색으로 그 키워드로 후보를 좁히고, 가장 적합한 세션을 골라 그 자리에서 resume 한다.

## 언제 호출하는가

- "그때 했던 작업 다시 시작해줘"
- "이전 세션 어디였지"
- "어제 / 지난주에 X 하던 거 이어서"
- "deno 인증 헤더 했던 그 세션"
- "session resume"
- "claude / codex 에서 한 그 작업 다시"
- "fork 안 해놨는데 그 작업 다시 켜고 싶다"

## 동작 순서

### 1) 키워드 매칭

```bash
recall search "<사용자 키워드>"
```

결과 0건이면 키워드를 변형해서 1~2번 더 (동의어, 영문↔한글, 단어 분해).

### 2) 단일 매칭이면 즉시 resume

```bash
recall resume "<사용자 키워드>"
```

`recall` 이 알아서 `claude --resume <uuid>` 또는 `codex resume <uuid>` 를 원래 cwd 에서 실행한다. 너는 추가 작업 없음.

### 3) 여러 후보면 사용자 확인

후보가 여럿이면:
1. `recall search` 출력의 각 후보 정보 (provider, session_id_prefix, title, last_ts, 본문 발췌) 를 사용자에게 보여준다.
2. 어느 세션인지 묻는다.
3. 확정되면 `recall resume <session_id_prefix>` 로 실행.

### 4) 연관 세션

사용자가 "비슷한 다른 세션도" 라고 하면:

```bash
recall related <session_id_prefix>
```

같은 cwd 의 다른 세션을 1-hop 그래프로 보여준다.

## 자동 인덱싱

`recall daemon install` 이 OS 스케줄러에 30분 주기 `recall scan` 을 등록해둔다. 사용자가 별도 요청하지 않으면 너는 `recall scan` 을 임의로 호출하지 않는다 (스케줄러가 알아서 하므로).

방금 한 세션을 너가 호출하기 직전 DB 에 들어가있어야 한다면, **사용자 권한 받고** 한 번:

```bash
recall scan
```

## 도구 부재 시

`recall` 이 PATH 에 없으면 사용자에게 안내하고 권한 받은 후 설치:

```bash
git clone https://github.com/Hostingglobal-Tech/recall.git ~/.local/share/recall
cd ~/.local/share/recall && cargo build --release
cp target/release/recall ~/.local/bin/
mkdir -p ~/.claude/skills/recall
cp plugins/claude/SKILL.md ~/.claude/skills/recall/SKILL.md
recall init && recall scan && recall daemon install
```

## 절대 하지 않을 일

- `recall resume` 실행은 사용자가 명확히 확정한 후에만. 단일 매칭이라도 한 번은 "이거 맞아?" 확인 권장.
- `recall scan --force` 를 임의로 호출 안 함 (성능 부담).
- 외부 API key / 비밀번호 / 자격 증명을 절대 입출력 하지 않음 (recall 자체가 그런 것 필요 없음).

## 참조

- README: https://github.com/Hostingglobal-Tech/recall
- 명령 목록: `recall --help`
