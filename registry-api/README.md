# registry-api

Dambi private adapter-registry 앞단의 **caching authenticated reverse-proxy**.
비공개 GCS 버킷 (`dambi-registry-seoul`, `asia-northeast3`) 을 Cloud Run 서비스
계정 권한으로 읽어 익스텐션에 중계한다. 익스텐션은 이 서비스 URL 을
`REGISTRY_BASE_URL` 로 가리킨다.

Spec: `ADAPTER_LOADER_ARCHITECTURE.md` §6.

## Endpoints

| Method | Path | 역할 |
|---|---|---|
| GET | `/health` | 헬스 체크 (`{ok:true}`) |
| GET | `/index/by-callkey/<chain>__<to>__<selector>.json` | adapter bundle index proxy |
| GET | `/tokens/<chain>/<address>.json` | token metadata proxy |
| GET | `/v1/registry/by-callkey?chain_id&to&selector` | §6.1 query alias (secondary) |
| GET | `/debug/recent` | 최근 요청 log + cache stats |
| OPTIONS | `*` | CORS preflight (204) |

## Proxy 계약

| 상황 | 응답 |
|---|---|
| GCS object found | 200 + body + `Cache-Control` + CORS |
| GCS object 없음 | **404** (익스텐션 negative-cache 가 의존하는 real status) |
| GCS upstream error | 502 |
| per-IP rate 초과 | 429 |
| bad path / method | 404 / 405 |

## DoS 완화

- in-memory LRU+TTL 캐시 — 동일 callkey 폭주를 RAM 에서 처리, GCS read 생략
- per-IP token-bucket rate limiter (instance-local — 한계는 `src/rate-limiter.ts` 주석 참조)
- 비용 상한은 Cloud Run `--max-instances`

## 로컬 개발

```bash
npm install
npm run typecheck
npm test
npm run build && npm start          # 실제 버킷 대상 실행은 ADC 필요
                                     # (gcloud auth application-default login)
```

## Docker / Cloud Run

```bash
docker build -f Dockerfile -t dambi-registry-api .
gcloud builds submit . --tag <REGION>-docker.pkg.dev/<PROJECT>/dambi/registry-api:v1
```

## 환경변수

`HOST` · `PORT` · `REGISTRY_BUCKET` · `CACHE_TTL_MS` · `CACHE_NEGATIVE_TTL_MS` ·
`CACHE_MAX_ENTRIES` · `CACHE_CONTROL` · `RATE_LIMIT_BURST` ·
`RATE_LIMIT_REFILL_PER_SEC` · `RATE_LIMIT_MAX_IPS` — 기본값은 `src/config.ts`.
