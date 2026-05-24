# ScopeBall Adapter Marketplace Registry (PoC)

ScopeBall 의 Adapter Function Bundle 을 호스팅하는 정적 파일 레지스트리. PoC 단계는 단일 endpoint + HTTPS + 정적 JSON 으로 시작 (spec [§6](../../ADAPTER_MARKETPLACE_ARCHITECTURE.md#6-registry-—-데모-단순-구현)).

## 디렉토리 트리

```
scopeball/registry/
├── manifests/                                              # Adapter Function Bundle 의 단일 진실
│   └── <publisher>/<protocol>/<func>@<version>.json
│       e.g. manifests/uniswap/v2/swapExactTokensForTokens@1.0.0.json
│
├── index/                                                  # build-index.ts 가 생성 (commit)
│   └── by-callkey/
│       └── <chain_id>__<lowercased_to>__<lowercased_selector>.json
│           ─ matched: true
│           ─ bundle_id, manifest_path
│           ─ bundle_sha256 = "0x" + hex(sha256(canonical_json(bundle)))  (RFC 8785 JCS)
│           ─ bundle: <full bundle JSON inlined>          # client 1-step lookup 용
│
├── scripts/
│   └── build-index.ts                                     # 인덱스 빌더 (tsx)
├── package.json
├── .gitignore
└── README.md
```

## 사용법

### 인덱스 빌드

```bash
cd scopeball/registry
npm install              # 또는 yarn install
npm run build            # manifests/ scan → bundle_sha256 계산 → index/ 재생성
```

`npm run build` 가 하는 일:
1. `manifests/**/*.json` 의 모든 Adapter Function Bundle 을 읽음
2. 각 bundle 의 `match.{chain_ids[], to[], selector}` cross product 로 callkey 조합
3. RFC 8785 JCS 로 canonicalize 후 `sha256` → `bundle_sha256`
4. `index/by-callkey/<chain_id>__<lowercased_to>__<lowercased_selector>.json` 생성
5. 기존 `index/by-callkey/` 는 wipe 후 재기록 (orphan 방지)

### 로컬 정적 호스팅

```bash
npm run serve            # python3 -m http.server 8000
# 또는 임의의 정적 서버 (nginx, caddy, static-server 등)
```

### Lookup

Client (browser-extension) 는 다음 패턴으로 GET:

```
GET http://localhost:8000/index/by-callkey/<chain_id>__<lowercased_to>__<lowercased_selector>.json
```

* 200 → JSON body 의 `bundle` field 사용 (inlined). `bundle_sha256` 로 client 측 무결성 검증 (spec [§7.3](../../ADAPTER_MARKETPLACE_ARCHITECTURE.md#73-코드-흐름-typescript-pseudo))
* 404 → registry 에 entry 없음. negative cache 5분 (`reason="no_publisher"`)

## spec §6.1 의 API endpoint 와의 mapping

spec §6.1 은 query parameter 형식 `GET /v1/registry/by-callkey?chain_id=&to=&selector=` 을 설명. 정적 호스팅 환경에서는 client 가 직접 다음 path 로 매핑:

```
/v1/registry/by-callkey?chain_id=X&to=Y&selector=Z
  ↓ (client 측 path 구성)
/index/by-callkey/<X>__<Y.toLowerCase()>__<Z.toLowerCase()>.json
```

운영 환경 (CDN / Cloudflare Pages) 에서는 rewrite rule 로 `/v1/registry/by-callkey` query path 를 위 정적 path 로 매핑하면 호환 가능. PoC 는 client 측 직접 매핑만.

## 데모 단계의 단순화

| 항목 | PoC | 향후 (spec §10) |
|---|---|---|
| Bundle 무결성 | `bundle_sha256` 비교 (client 측) | + ED25519 signature + Sigstore + Sourcify hash binding (§10.1) |
| Publisher 신원 | 메타데이터 표시만 | ENS / ERC-1271 / DNS attestation (§10.2) |
| Revocation | 없음 | m-of-n signed revocation feed (§10.3) |
| Multi-mirror | 단일 endpoint | Cloudflare + AWS + IPFS (§10.1) |
| Rate limit | 없음 | per-IP throttling (§10.1) |
| 호스팅 | local dev (`python3 -m http.server`) | GCP / Cloudflare Pages / GitHub Pages |

## 신규 Adapter Function 추가 절차

1. `manifests/<publisher>/<protocol>/<func>@<version>.json` 작성 (spec §4.x 의 4 strategy 중 하나)
2. `npm run build` → index 재생성
3. `git add manifests/ index/ && git commit`

PoC 단계는 build output (`index/by-callkey/`) 도 commit 한다 (정적 호스팅 즉시 가능). CI 빌드로 전환은 향후 작업.
