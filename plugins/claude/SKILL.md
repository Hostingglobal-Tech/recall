---
name: recall
description: 사용자가 과거 Claude Code 또는 Codex 세션을 흐릿한 기억으로 다시 찾고 그 자리에서 작업을 이어가려 할 때 호출. "그때 했던 작업 다시", "이전 세션 어디였지", "X 작업 했던 세션 찾아", "session resume", "fork 안 해놨는데 그거 다시", "어제 한 그거 이어서" 같은 표현 감지 시 사용. CLI 도구 `recall` 이 시스템에 설치되어 있어야 함.
---

# recall — 과거 세션 재발견 + resume

사용자는 일반적으로 세션 id 를 기억하지 못합니다. 흐릿한 주제 키워드만 알려줍니다. 너는 `recall` CLI 를 통해 그 키워드로 후보를 좁히고, 가장 적합한 세션을 골라 그 자리에서 resume 한다.

## 언제 호출하는가

사용자가 다음과 같이 말할 때:

- "그때 했던 작업 다시 시작해줘"
- "이전 세션 어디였지"
- "어제 / 지난주에 X 하던 거 이어서"
- "deno 인증 헤더 했던 그 세션"
- "session resume"
- "claude/codex 에서 한 그 작업 다시"
- "fork 안 해놨는데 그 작업 다시 켜고 싶다"

## 동작 순서

### 1) 키워드 매칭

```bash
recall search "<사용자 키워드>"
```

후보 목록을 본다. 결과 비어있으면 키워드를 변형/추측해서 1~2번 더 시도 (동의어, 영문→한글 또는 반대).

### 2) 단일 매칭이면 즉시 resume

후보가 명확히 하나면:

```bash
recall resume "<사용자 키워드>"
```

`recall` 이 알아서 `claude --resume <uuid>` 또는 `codex resume <uuid>` 를 원래 cwd 에서 실행한다. 너는 추가 작업 없음.

### 3) 여러 후보면 사용자 확인

후보가 여럿이면:
1. 각 후보의 `session_id_prefix`, `provider`, `title`, `last_ts`, 본문 발췌를 사용자에게 보여준다 (이미 `recall search` 출력에 포함됨).
2. 사용자에게 어느 세션인지 물어본다.
3. 확정되면 `recall resume <session_id_prefix>` 로 실행한다.

### 4) 의미 검색 fallback

`recall search` 가 0건이고 사용자가 키워드를 더 흐릿하게만 기억할 때, 임베딩이 미리 설정되어 있으면:

```bash
recall semantic "<더 풀어 쓴 자연어 설명>"
```

(설정 안 되어 있으면 README 의 의미검색 섹션을 짚어주고 권한 받은 뒤 설정.)

### 5) 연관 세션

사용자가 "비슷한 다른 세션도 보여줘" 라고 하면:

```bash
recall related <session_id_prefix>
```

같은 cwd 의 다른 세션을 1-hop 그래프로 보여준다.

## 도구 부재 시

`recall` 이 PATH 에 없으면 사용자에게 한 줄로 안내하고 권한 받은 후 설치:

```bash
# Rust 가 있다면
git clone https://github.com/Hostingglobal-Tech/recall.git ~/.local/share/recall
cd ~/.local/share/recall && cargo build --release
cp target/release/recall ~/.local/bin/

# 첫 인덱싱
recall init && recall scan
```

이후 본 세션 작업 진행.

## 절대 하지 않을 일

- 사용자가 명시적으로 요청하지 않으면 임의로 `recall scan --force` 호출 안 함 (성능 부담).
- 임베딩 API 키를 직접 코드/터미널에 출력 안 함 (사용자가 본인 `~/.recall/config.toml` 에 환경변수명만 적도록 유도).
- `recall resume` 실행은 사용자가 명확히 확정한 후에만. 단일 매칭이라도 사용자가 "이거 맞아?" 단계 한 번은 거치는 게 좋다.

## 참조

- README: https://github.com/Hostingglobal-Tech/recall
- 명령 전체 목록: `recall --help`
