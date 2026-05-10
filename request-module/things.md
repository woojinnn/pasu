# ScopeBall — RPC Field Extractor

## 0. TL;DR

dApp이 MetaMask에 보내는 RPC 요청을 가로채서 `method / chainId / from / to / calldata / value / gas` 같은 공통 필드만 **얕게 추출**하고, 그 결과를 ScopeBall(crates/web-server)의 ABI Resolver UI에 자동으로 채워 넣는다. 의미해석/ABI decode는 이미 백엔드(`abi-resolver` crate)가 담당하므로 여기서는 **요청 필드 수집기**만 만든다.

최종 목표는 크롬 확장 1개로 자기완결시키는 것이지만, 우선은 기존 web-server + 프런트엔드 인프라 위에서 동작 검증 먼저 한 뒤 확장으로 옮긴다.

---

## 1. 단계적 접근 (Phase 1 → Phase 2)

### 1.1 Phase 1 — localhost:5173 시각화 (지금 만들 것)

```
┌─ dApp page (Uniswap 등) ─────────────────────────────────┐
│  Tampermonkey 유저스크립트                                │
│   ├ window.ethereum.request 후킹                         │
│   ├ extractRpcFields()                                   │
│   └ GM_xmlhttpRequest("POST http://127.0.0.1:8080/api/event") │
└──────────────────────────────────────────────────────────┘
                             ↓
┌─ crates/web-server (Axum, :8080) ──────────────┐
│  POST /api/event       ← 추출된 JSON 수신                  │
│  GET  /api/event/stream (SSE) ← 프런트엔드에 push         │
│  POST /api/decode      ← 기존 ABI 디코드 그대로            │
└──────────────────────────────────────────────────────────┘
                             ↓ SSE
┌─ frontend (Vite, :5173) ─────────────────────────────────┐
│  EventSource("/api/event/stream")                        │
│   └ 받은 즉시 chainId / to / calldata 폼 자동 채움         │
│   └ (선택) 자동 Decode                                    │
└──────────────────────────────────────────────────────────┘
```

### 1.2 Phase 2 — Chrome Extension (나중에)

```
┌─ dApp page ──────────────────────┐
│  injected.js (page world)        │
│   ├ window.ethereum.request hook │
│   └ window.postMessage           │
│  content.js (isolated world)     │
│   └ chrome.runtime.sendMessage   │
└──────────────────────────────────┘
                ↓
┌─ background.js (MV3 service worker) ┐
│  - extractRpcFields                  │
│  - ABI decode (Sourcify + viem)      │
│  - chrome.storage 캐시               │
└──────────────────────────────────────┘
                ↓
┌─ Side Panel UI ──────────────────┐
│  React (Phase 1 컴포넌트 재사용)  │
└──────────────────────────────────┘
```

### 1.3 마이그레이션 비용

| 영역 | Phase 1 | Phase 2 | 재사용 |
|---|---|---|---|
| 추출 로직 (`extractRpcFields`) | core/ 에 순수 함수 | 동일 모듈 import | **100%** |
| ABI 디코딩 | 기존 `abi-resolver` crate 호출 | 별도 (viem 또는 wasm화한 abi-resolver) | 0~70% |
| React UI | 기존 `DecodeForm` / `DecodeResult` | side panel에 마운트 | **~90%** |
| transport | HTTP fetch + SSE | chrome.runtime 메시지 | 0% |
| 빌드 | 추가 없음 (유저스크립트 단일 파일) | Vite + @crxjs/vite-plugin | — |

**핵심 원칙**: 추출 로직은 환경에 종속되지 않게 분리해서 `transport` 어댑터만 갈아끼우면 옮겨갈 수 있게 한다.

### 1.4 폴더 구조 (request-module/)

```
request-module/
├── things.md                       (이 문서)
├── core/
│   └── extractRpcFields.ts        ← 순수 함수, 어디서든 동일
├── userscript/
│   └── scopeball.user.js          ← Phase 1 dApp 후킹용
└── README.md                       ← 사용법
```

`core/extractRpcFields.ts`의 로직은 Phase 2에서도 그대로 import해서 쓰면 된다.

---

## 2. 파싱 대상 필드

### 2.1 항상 추출

| 필드 | 설명 |
|---|---|
| `method` | RPC method 이름 (`args.method`) |
| `origin` | 요청 발신 페이지 origin (`location.origin`) |
| `currentChainId` | provider의 현재 chainId (params에 없을 때 폴백) |

### 2.2 키 화이트리스트 (재귀 탐색)

#### 주소 → `addresses[]` (단일 필드 `from`, `to`도 별도 저장)
- `from`, `to`
- `address`, `account`
- `owner`, `spender`
- `recipient`, `sender`
- `verifyingContract`, `contractAddress`

#### chainId → `chainIds[]` (숫자/hex 모두 hex로 정규화)
- `chainId`
- `domain.chainId` (typedData)

#### calldata → `calldata[]`
- `data`, `input`, `calldata`, `callData`

#### value → `value`
- `value` (hex 문자열)

#### gas 계열 → `gasFields{}`
- `gas`, `gasLimit`, `gasPrice`
- `maxFeePerGas`, `maxPriorityFeePerGas`

### 2.3 값 휴리스틱 (key 없이 값만)
- `^0x[a-fA-F0-9]{40}$` → 주소로 간주
- `^0x[a-fA-F0-9]+$` && length ≥ 10 → calldata 후보 (선택)

### 2.4 결과 스키마

```ts
type ExtractedRpcFields = {
  method: string;
  origin?: string;
  currentChainId?: string;
  primaryChainId?: string;     // chainIds[0] || currentChainId
  chainIds: string[];
  addresses: string[];
  from?: string;
  to?: string;
  value?: string;
  calldata: string[];
  gasFields: Record<string, string>;
  rawParams: unknown;
  parsedTypedData?: unknown;   // typedData 문자열 파싱 시 결과
};
```

---

## 3. method별 커버리지

| method | 추출 가능한 필드 |
|---|---|
| `eth_sendTransaction` | from, to, data, value, gas* |
| `eth_call` | to, data |
| `eth_estimateGas` | to, data, gas* |
| `wallet_switchEthereumChain` | chainId |
| `wallet_addEthereumChain` | chainId, (rpcUrls/blockExplorerUrls — 별도 처리) |
| `eth_signTypedData_v4` | signer address, domain.chainId, domain.verifyingContract |
| `personal_sign` | signer address |
| 알 수 없는 method | 안에 주소/data/chainId 있으면 추출, 없으면 method만 |

---

## 4. 예시

### 입력
```ts
await window.ethereum.request({
  method: "eth_sendTransaction",
  params: [{
    from: "0x1111111111111111111111111111111111111111",
    to:   "0x2222222222222222222222222222222222222222",
    value: "0x0",
    data:  "0x095ea7b30000000000000000000000003333333333333333333333333333333333333333",
    gas:   "0x5208",
    maxFeePerGas: "0x59682f00",
    chainId: "0x1",
  }],
});
```

### 추출 결과
```json
{
  "method": "eth_sendTransaction",
  "origin": "https://app.uniswap.org",
  "primaryChainId": "0x1",
  "chainIds": ["0x1"],
  "addresses": [
    "0x1111111111111111111111111111111111111111",
    "0x2222222222222222222222222222222222222222"
  ],
  "from":  "0x1111111111111111111111111111111111111111",
  "to":    "0x2222222222222222222222222222222222222222",
  "value": "0x0",
  "calldata": [
    "0x095ea7b30000000000000000000000003333333333333333333333333333333333333333"
  ],
  "gasFields": {
    "gas": "0x5208",
    "maxFeePerGas": "0x59682f00"
  }
}
```

---

## 5. Phase 1 동작 흐름 (상세)

```
1. dApp이 window.ethereum.request(args) 호출
2. 유저스크립트가 page world에 inject한 hook이 가로챔
3. extractRpcFields(args) 실행
4. window.postMessage로 sandbox 컨텍스트(유저스크립트 본체)에 전달
5. 유저스크립트가 GM_xmlhttpRequest로 POST /api/event
6. Axum이 broadcast::Sender로 fanout
7. SSE 구독 중인 프런트엔드가 EventSource onmessage로 수신
8. App이 chainId / to / calldata를 form state에 주입
9. (옵션) 자동으로 /api/decode 호출 → DecodeResult 표시
10. 원본 request는 변형 없이 MetaMask로 forward
```

> 9번 자동 decode는 **Resolved**가 나오면 바로 의미해석까지 한 화면에 보여주는 흐름. 처음엔 끄고 폼만 자동 채움 → 사용자가 Decode 버튼 누르는 식으로 시작.

---

## 6. Phase 1 단계별 로드맵

### Step 1: 코어 추출 로직
- `request-module/core/extractRpcFields.ts` 작성
- 단위 테스트는 일단 생략 (브라우저에서 실측 검증)

### Step 2: 유저스크립트
- `request-module/userscript/scopeball.user.js` 작성
- core 로직을 인라인으로 포함 (빌드 단계 없음)
- page-world inject + sandbox postMessage + GM_xmlhttpRequest 패턴

### Step 3: 백엔드 수신/스트림
- `crates/web-server/src/main.rs`에 추가:
  - `POST /api/event` — JSON 수신, broadcast로 fanout
  - `GET /api/event/stream` — SSE
- 의존성: `tokio-stream`(broadcast → Stream 변환), 기존 axum/serde로 충분
- `Cargo.toml` 업데이트

### Step 4: 프런트엔드 수신/자동 채움
- `App.tsx`에서 form state lift up (chainId/address/calldata를 App에서 보유)
- `DecodeForm`에 `value`/`onChange` props 추가하여 controlled 컴포넌트로
- `useEffect`에서 `EventSource("/api/event/stream")` 구독
- 이벤트 받으면 `setChainId(payload.primaryChainId 16진수→10진수)`, `setAddress(payload.to)`, `setCalldata(payload.calldata[0])`

### Step 5: 검증
- web-server 실행 (`cargo run -p web-server`)
- frontend dev (`cd frontend && npm run dev`)
- 유저스크립트 Tampermonkey에 등록
- 테스트 dApp(예: app.uniswap.org)에서 swap 시도
- :5173 폼이 자동으로 채워지는지 확인

---

## 7. Phase 2 단계별 로드맵 (예고)

1. Vite + `@crxjs/vite-plugin` 셋업
2. `injected.js` (page world hook) — Step 2의 inline 부분 그대로
3. `content.js` (isolated world) — postMessage → chrome.runtime 메시지 릴레이
4. `background.js` (MV3 service worker) — 메시지 수신 + decode (abi-resolver wasm 또는 viem)
5. Side Panel — Phase 1의 React 컴포넌트 마운트 위치만 변경
6. `chrome.storage`로 영속화

핵심 코드(extractRpcFields, DecodeForm, DecodeResult)는 그대로 옮겨감.

---

## 8. 참고 구현 (extractRpcFields 초안)

```ts
type RpcRequest = {
  method: string;
  params?: unknown[] | Record<string, unknown>;
};

const ADDRESS_KEYS = new Set([
  "address", "account", "owner", "spender",
  "recipient", "sender", "verifyingContract", "contractAddress",
]);
const CALLDATA_KEYS = new Set(["data", "input", "calldata", "callData"]);
const GAS_KEYS = new Set([
  "gas", "gasLimit", "gasPrice", "maxFeePerGas", "maxPriorityFeePerGas",
]);

function isAddress(v: unknown): v is string {
  return typeof v === "string" && /^0x[a-fA-F0-9]{40}$/.test(v);
}
function isHexData(v: unknown): v is string {
  return typeof v === "string" && /^0x[a-fA-F0-9]*$/.test(v);
}
function looksLikeCalldata(v: unknown): v is string {
  return typeof v === "string" && /^0x[a-fA-F0-9]+$/.test(v) && v.length >= 10;
}
function normalizeChainId(v: unknown): string | null {
  if (typeof v === "string") {
    if (/^0x[0-9a-fA-F]+$/.test(v)) return v.toLowerCase();
    const n = Number(v);
    return Number.isFinite(n) ? "0x" + n.toString(16) : null;
  }
  if (typeof v === "number") return "0x" + v.toString(16);
  return null;
}

function extractRpcFields(args: RpcRequest, opts?: { origin?: string; currentChainId?: string }) {
  const result = {
    method: args.method,
    origin: opts?.origin,
    currentChainId: opts?.currentChainId,
    primaryChainId: undefined as string | undefined,
    chainIds: [] as string[],
    addresses: [] as string[],
    from: undefined as string | undefined,
    to: undefined as string | undefined,
    value: undefined as string | undefined,
    calldata: [] as string[],
    gasFields: {} as Record<string, string>,
    rawParams: args.params ?? [],
    parsedTypedData: undefined as unknown,
  };

  function visit(value: unknown, key?: string) {
    if (value == null) return;

    if (typeof value === "string") {
      // typedData 등 JSON-string 처리: { … } 패턴이면 parse 시도
      const trimmed = value.trim();
      if (trimmed.startsWith("{") && trimmed.endsWith("}")) {
        try {
          const parsed = JSON.parse(trimmed);
          if (key === undefined || key === "1" || key === "0") {
            result.parsedTypedData = parsed;
          }
          visit(parsed);
          return;
        } catch { /* not JSON */ }
      }
    }

    if (typeof value === "string" || typeof value === "number") {
      if (key === "chainId") {
        const c = normalizeChainId(value);
        if (c) result.chainIds.push(c);
      }
      if (key === "from" && isAddress(value)) {
        result.from = value;
        result.addresses.push(value);
      }
      if (key === "to" && isAddress(value)) {
        result.to = value;
        result.addresses.push(value);
      }
      if (key && ADDRESS_KEYS.has(key) && isAddress(value)) {
        result.addresses.push(value);
      }
      if (key && CALLDATA_KEYS.has(key) && looksLikeCalldata(value)) {
        result.calldata.push(value);
      }
      if (key === "value" && isHexData(value)) {
        result.value = value;
      }
      if (key && GAS_KEYS.has(key)) {
        result.gasFields[key] = String(value);
      }
      // key 없어도 값이 주소처럼 생기면 수집 (personal_sign 등)
      if (typeof value === "string" && isAddress(value)) {
        result.addresses.push(value);
      }
      return;
    }

    if (Array.isArray(value)) {
      for (const item of value) visit(item);
      return;
    }
    if (typeof value === "object") {
      for (const [k, v] of Object.entries(value)) visit(v, k);
    }
  }

  visit(args.params);

  result.chainIds  = [...new Set(result.chainIds)];
  result.addresses = [...new Set(result.addresses)];
  result.calldata  = [...new Set(result.calldata)];
  result.primaryChainId = result.chainIds[0] ?? result.currentChainId;
  return result;
}
```

---

## 9. 차후 수정해야 할 부분 (TODO)

> Phase 1 동작 검증 후 실배포/Phase 2 이전에 손볼 것.

### 9.1 [기존 8.1 — 부분 반영됨] typedData 문자열 파싱
- §8 초안에 `parsedTypedData` 시도 로직 추가했지만, `eth_signTypedData_v4`는 항상 두 번째 param이 JSON 문자열이라는 보장이 있어서 **method별 강제 파싱 분기**도 추가하면 더 견고함.

### 9.2 [기존 8.2] chainId 폴백 자기 재후킹
- `currentChainId` 폴백을 위해 `provider.request({method:"eth_chainId"})` 부르면 안 됨. 후킹 시 `originalRequest`를 캡처해두고 그걸 호출.

### 9.3 [기존 8.3 — 해결됨] dead branch
- §8 초안에서 단순화 완료.

### 9.4 [기존 8.4] postMessage targetOrigin
- 유저스크립트 page-world inject 시 `window.postMessage(payload, location.origin)`로 origin 명시. sandbox 측에서 `event.origin === location.origin`으로 검증.

### 9.5 [기존 8.5] provider hook 견고성
- `window.ethereum`이 늦게 주입되는 케이스: `ethereum#initialized` 이벤트 + 짧은 polling fallback.
- 재할당이 안 먹히면 `Object.defineProperty`로 `request`를 재정의 시도.

### 9.6 [기존 8.6] EIP-6963 multi-injected provider
- `eip6963:announceProvider` 이벤트 수신해서 모든 provider에 hook 적용.

### 9.7 [기존 8.7] primaryChainId — 부분 반영됨
- §8 초안에서 단순히 `chainIds[0] ?? currentChainId`로 정함. `eth_signTypedData_v4`의 `domain.chainId`와 외부 chainId가 다를 때 우선순위 정책 다듬을 것.

### 9.8 [기존 8.8] `wallet_addEthereumChain` 부가 정보
- `rpcUrls`, `blockExplorerUrls`, `nativeCurrency` 별도 키 추가 추출.

### 9.9 [기존 8.9] 추출 실패/부분 추출 플래그
- `extractionFlags: { hasAddress, hasCalldata, hasChainId, parsedTypedData }` 추가.

### 9.10 [신규] 유저스크립트 inject 타이밍
- `@run-at document-start` 필수. 일부 dApp(특히 SPA)이 `window.ethereum`을 lazy하게 사용하므로 hook이 사용 시점보다 먼저 들어가야 함.

### 9.11 [신규] /api/event 인증/제한
- 현재는 CORS permissive + 인증 없음. 로컬 개발 한정 OK지만 외부 노출 시 토큰/origin 화이트리스트 필요.

### 9.12 [신규] SSE 재연결 로직
- 프런트엔드 `EventSource`는 자동 재연결되지만, 백엔드 broadcast 채널이 가득 차서 lagged 되는 경우 재구독 처리.
