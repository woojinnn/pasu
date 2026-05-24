/**
 * registry-api — request-path validation.
 *
 * proxy 는 임의 버킷 object 에 요청이 닿게 두면 안 된다. 익스텐션이 GET 하는
 * 경로는 딱 두 종류:
 *   GET /index/by-callkey/<chainId>__<to>__<selector>.json
 *   GET /tokens/<chainId>/<address>.json
 *
 * 이 regex 는 browser-extension/backend/service-worker/registry/client.ts
 * (CALL_KEY_ADDRESS_RE / CALL_KEY_SELECTOR_RE) 를 미러. 단 to/address/selector
 * 를 LOWERCASE 만 허용 — index builder (registry/scripts/build-index.ts
 * `callkeyFilename`) 가 소문자 object 이름을 쓰고 GCS object 이름은
 * case-sensitive 라서다. 그 외 전부 → 404 (버킷 read 안 함). 404 (400 아님)
 * 라야 익스텐션 negative-cache 계약이 유지된다.
 */
const ADDRESS_LC = "0x[0-9a-f]{40}";
const SELECTOR_LC = "0x[0-9a-f]{8}";
const CHAIN_ID = "[1-9][0-9]*";

const CALL_KEY_RE = new RegExp(`^${CHAIN_ID}__${ADDRESS_LC}__${SELECTOR_LC}$`);
const CHAIN_RE = new RegExp(`^${CHAIN_ID}$`);
const ADDRESS_RE = new RegExp(`^${ADDRESS_LC}$`);

export function isValidCallKeySegment(s: string): boolean {
  return CALL_KEY_RE.test(s);
}
export function isValidChainSegment(s: string): boolean {
  return CHAIN_RE.test(s);
}
export function isValidAddressSegment(s: string): boolean {
  return ADDRESS_RE.test(s);
}

export interface ProxyTargetOk {
  ok: true;
  objectName: string;
}
export interface ProxyTargetErr {
  ok: false;
}
export type ProxyTarget = ProxyTargetOk | ProxyTargetErr;

const INDEX_PREFIX = "/index/by-callkey/";
const TOKENS_PREFIX = "/tokens/";
const JSON_SUFFIX = ".json";

/**
 * 요청 pathname → 비공개 버킷 object 이름, 또는 reject.
 * Defence in depth: ".." 또는 "%" 가 든 pathname 도 reject.
 */
export function parseProxyTarget(pathname: string): ProxyTarget {
  if (pathname.includes("..") || pathname.includes("%")) return { ok: false };

  if (pathname.startsWith(INDEX_PREFIX) && pathname.endsWith(JSON_SUFFIX)) {
    const seg = pathname.slice(
      INDEX_PREFIX.length,
      pathname.length - JSON_SUFFIX.length,
    );
    return isValidCallKeySegment(seg)
      ? { ok: true, objectName: `index/by-callkey/${seg}.json` }
      : { ok: false };
  }

  if (pathname.startsWith(TOKENS_PREFIX) && pathname.endsWith(JSON_SUFFIX)) {
    const inner = pathname.slice(
      TOKENS_PREFIX.length,
      pathname.length - JSON_SUFFIX.length,
    );
    const slash = inner.indexOf("/");
    if (slash <= 0) return { ok: false };
    const chain = inner.slice(0, slash);
    const address = inner.slice(slash + 1);
    return isValidChainSegment(chain) &&
      isValidAddressSegment(address) &&
      !address.includes("/")
      ? { ok: true, objectName: `tokens/${chain}/${address}.json` }
      : { ok: false };
  }
  return { ok: false };
}
