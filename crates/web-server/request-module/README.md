# scopeball + request-module — 전체 사용 가이드

처음 셋업부터 dApp 사용까지 5단계.

```
[1] Repo clone           (한 번)
[2] DB 빌드              (한 번, ~1시간)
[3] 백엔드 시작           (매번)
[4] 프론트 시작           (매번)
[5] userscript 설치       (한 번)
   ↓
실제 사용 (dApp에서 swap)
```

---

## ① Repo clone — 처음 한 번

```bash
git clone https://github.com/woojinnn/scopeball.git
cd scopeball
```

받은 후엔:
- Rust 코드 (백엔드 + 디코더)
- React 코드 (프론트)
- userscript 코드 (브라우저 hook)
- 큐레이션 데이터 (`crates/abi-resolver/data/sourcify.json`, 125 KB)

→ DB는 안 받음 (`.gitignore`). 다음 단계에서 빌드.

---

## ② DB 빌드 — 처음 한 번 (~1시간)

```bash
./crates/abi-resolver/scripts/build_all.sh
```

이 스크립트가 자동으로:

1. Python venv 만들고 (`/tmp/parquet_venv`)
2. Sourcify dump 24 GB 받고 (`export.sourcify.dev`)
3. mainnet 매핑 추출
4. SQLite DB 빌드 (`/tmp/sourcify_dump/sourcify.sqlite`, ~9 GB)
5. 빌드 캐시 삭제

→ 끝나면 **9 GB DB만 남음**. 모든 메인넷 verified contract의 함수 ABI.

> 비유: 식당 차리는데 **재료 도매시장에서 한 트럭 사와서 주방 정리** 하는 단계. 한 번만 하면 됨.

플래그:
- `--force` — DB 이미 있어도 재빌드
- `--purge` — 빌드 후 venv + mapping 까지 정리

---

## ③ 백엔드 시작 — 매번 (Rust 서버)

```bash
WEB_SERVER_ADDR=127.0.0.1:8080 cargo run -p web-server
```

이게 하는 일:
- Rust 코드 컴파일 (처음만, 이후 캐시)
- HTTP API 서버 시작 (`localhost:8080`)
- SQLite DB 자동 부착 (있으면)
- 큐레이션 JSON 메모리 로드

```
사용 가능한 endpoint:
  POST /api/decode         calldata → 디코드
  POST /api/event          userscript가 RPC 이벤트 보내는 곳
  GET  /api/event/stream   프론트가 SSE로 구독
  GET  /api/health         alive check
```

→ 띄워두는 동안 동작. `Ctrl+C`로 종료.

> 비유: **주방 직원이 자기 자리에서 대기**. 영수증 들어오면 처리.

---

## ④ 프론트 시작 — 매번 (React + Vite)

```bash
cd crates/web-server/frontend
npm install   # 처음 한 번
npm run dev
```

이게 하는 일:
- Vite dev 서버 시작 (`localhost:5173`)
- HMR (Hot Module Reload) — 코드 수정 시 자동 새로고침
- `/api/*` 요청을 자동으로 백엔드(`:8080`)로 프록시

→ 브라우저에서 `http://localhost:5173` 열면 React 앱.

> 비유: **카운터/모니터링 화면 켜는 것**. 손님이 보는 매장 정면 인터페이스.

---

## ⑤ userscript 설치 — 처음 한 번

1. Chrome에 [Tampermonkey](https://www.tampermonkey.net/) 설치
2. Tampermonkey 대시보드 → "Create a new script"
3. `crates/web-server/request-module/userscript/scopeball.user.js` 내용 복사·붙여넣기 → 저장

→ 이제 **모든 dApp 페이지에서 자동 hook**.

> 비유: **모든 결제 카운터에 보안 카메라 설치**. 한 번 달면 영원히.

---

## 실제 사용 — dApp에서 swap 시 흐름

```
1. 브라우저로 https://app.uniswap.org 접속
   ↓ Tampermonkey가 자동으로 userscript 주입
   ↓ window.ethereum hook 잡음

2. swap 누름 (예: USDC → WETH)
   ↓ MetaMask가 트랜잭션 서명창 띄움
   ↓ 동시에 userscript가 RPC 가로챔
   ↓ POST http://127.0.0.1:8080/api/event {to, calldata, ...}

3. 백엔드가 받음 → SSE broadcast

4. localhost:5173 React 앱이 받음
   ↓ "Transactions" 섹션에 새 카드 [Use ↓]

5. [Use ↓] 클릭
   ↓ 폼이 자동 prefill (chain/to/calldata)

6. [Decode] 클릭
   ↓ POST /api/decode
   ↓ abi-resolver가 3-tier로 풀이:
     - 큐레이션 (15 contracts)
     - SQLite (1.4M contracts)
     - openchain seed
   ↓ 응답 {function_name, args[]}

7. 화면에 결과 표시
   - function: swapExactInputSingle
   - amount: 100 USDC
   - to: 0xb8bcee...
```

---

## 한눈에 보는 의존 그래프

```
                          [매번]
[한 번]                   ┌──────────────────────┐
build_all.sh ──→ 9GB DB ──→ cargo run ──→ Rust :8080
                          │                  ↑
                          │                  │ POST /api/event
                          │                  │
                          ↓                  │
[한 번]                   npm run dev ──→ Vite :5173
userscript install     ──────────────────→  ↑
       │                                    │ EventSource
       ↓                                    │ /api/event/stream
   브라우저 dApp ──── userscript ────────────┘
```

---

## 매번 띄우는 명령 (한 줄 요약)

```bash
# 터미널 1
WEB_SERVER_ADDR=127.0.0.1:8080 cargo run -p web-server

# 터미널 2
cd crates/web-server/frontend && npm run dev

# 브라우저
open http://localhost:5173
```

dApp에서 트랜잭션 시도 → 자동 prefill → Decode 클릭 → 결과 보임.

---

## 시간 / 디스크 견적

| 단계 | 시간 | 디스크 |
|---|---|---|
| ① clone | 30초 | ~50 MB (코드 + 큐레이션) |
| ② build_all.sh | ~1시간 | +9 GB (DB) |
| ③ cargo run (처음) | 5~10분 (컴파일) | +1~2 GB (target/) |
| ④ npm install (처음) | 1~2분 | +200 MB (node_modules) |
| ⑤ userscript install | 30초 | 0 (Tampermonkey 안) |
| 이후 매 실행 | cargo run < 5초, npm run dev < 1초 | (변동 X) |

**처음 셋업: 약 1.5시간**, 그 후 **매 실행 ~10초**.

---

## 디버그

| 증상 | 확인 |
|---|---|
| 폼이 안 채워진다 | 크롬 DevTools Console에서 `[scopeball] hooked` 로그 확인 |
| `[scopeball] POST failed` | 백엔드가 :8080에서 동작 중인지, CORS permissive인지 확인 |
| `[scopeball] hook error` | dApp이 비표준 provider를 쓰는 경우. Console 에러 본문 확인 |
| 클릭해도 아무 일 없음 | dApp이 `window.ethereum`을 lazy 로드 중. 새로고침 |
| `cargo run` 안 됨 | Rust 설치 확인 (`rustup`), `cargo build --workspace` 한 번 |
| `npm run dev` 안 됨 | Node.js 18+ 확인 |
| Decode 결과가 `arg0, arg1` | 큐레이션·SQLite에 없는 컨트랙트 → openchain fallback. SQLite DB 빌드됐는지 확인 |

---

## 폴더 구성

```
scopeball/
├── Cargo.toml                    Rust workspace
├── crates/
│   ├── policy-engine/
│   ├── adapters/...
│   ├── abi-resolver/             ★ Sourcify 디코더 + SQLite 빌더
│   │   ├── data/sourcify.json   (큐레이션, ~125 KB, commit됨)
│   │   ├── scripts/
│   │   │   ├── build_all.sh     (한 번 실행으로 끝)
│   │   │   ├── extract_mapping.py
│   │   │   ├── build_db.py
│   │   │   └── curate_bundle.sh
│   │   └── src/                 (Rust 라이브러리)
│   └── web-server/               ★ HTTP API + React 프론트 + 브라우저 hook
│       ├── src/main.rs           (axum)
│       ├── frontend/             (Vite + React + TS)
│       └── request-module/       ★ 브라우저 측 RPC hook
│           ├── core/extractRpcFields.ts (Phase 2 extension에 재사용)
│           └── userscript/scopeball.user.js   (Phase 1 Tampermonkey)
```

---

## Phase 2 (예정)

지금은 Tampermonkey 기반 (Phase 1). 다음 단계는:

- `crates/web-server/request-module/extension/` 추가
- `core/extractRpcFields.ts` 를 `inject.js`에서 import
- transport를 `chrome.runtime.sendMessage`로 교체
- frontend 컴포넌트(`DecodeForm`, `DecodeResult`)는 side panel에 마운트
- 사용자는 Tampermonkey 없이 우리 extension만 install하면 끝

자세한 설계는 [`things.md`](./things.md) §1.2, §7 참고.

---

## 한 줄 요약

> ① clone → ② `build_all.sh` (1시간) → ③ `cargo run` + ④ `npm run dev` → ⑤ userscript 설치 → dApp 사용. **②⑤만 한 번 하면 끝**, 매번은 ③④만.
