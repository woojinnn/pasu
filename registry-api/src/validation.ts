/**
 * registry-api — request-path validation.
 *
 * proxy 는 임의 버킷 object 에 요청이 닿게 두면 안 된다. 익스텐션이 GET 하는
 * 경로는 딱 세 종류:
 *   GET /index/by-callkey/<chainId>__<to>__<selector>.json
 *   GET /index/by-typed-data/<chainId>__<verifyingContract>__<primaryType>.json
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
const SHA256_LC = "0x[0-9a-f]{64}";
const SAFE_SEGMENT = "[a-z0-9._-]+";

const CALL_KEY_RE = new RegExp(`^${CHAIN_ID}__${ADDRESS_LC}__${SELECTOR_LC}$`);
// selector-only key = <chainId>__<selector.lower> (address-agnostic adapters,
// e.g. standard NFT setApprovalForAll). No address segment; both fragments are
// tightly bounded so it stays path-traversal-safe.
const SELECTOR_KEY_RE = new RegExp(`^${CHAIN_ID}__${SELECTOR_LC}$`);
const CHAIN_RE = new RegExp(`^${CHAIN_ID}$`);
const ADDRESS_RE = new RegExp(`^${ADDRESS_LC}$`);
const BUNDLE_FILE_RE = new RegExp(`^${SHA256_LC}\\.json$`);
// Detached bundle signature sidecar = <bundle_sha256>.sig (content-addressed,
// 0x + 64 lowercase hex). Tightly bounded → path-traversal-safe.
const SIG_FILE_RE = new RegExp(`^${SHA256_LC}\\.sig$`);

// typed-data key = <chainId>__<verifyingContract.lower>__<primaryType>.
// primaryType 는 EIP-712 콜론(:)이 "__" 로 escape 된 형태라 자체적으로 "__" 를
// 품을 수 있다 (예: HyperliquidTransaction:UsdSend → HyperliquidTransaction__UsdSend).
// 그래서 trailing 세그먼트는 [A-Za-z0-9_]+ — "/" / "." / ".." / 공백이 불가능해
// path-traversal-safe. chainId / verifyingContract 는 callkey 와 같은 fragment 재사용.
const TYPED_DATA_KEY_RE = new RegExp(
  `^${CHAIN_ID}__${ADDRESS_LC}__[A-Za-z0-9_]+$`,
);

export function isValidCallKeySegment(s: string): boolean {
  return CALL_KEY_RE.test(s);
}
export function isTypedDataKey(s: string): boolean {
  return TYPED_DATA_KEY_RE.test(s);
}
export function isValidSelectorKey(s: string): boolean {
  return SELECTOR_KEY_RE.test(s);
}
export function isValidChainSegment(s: string): boolean {
  return CHAIN_RE.test(s);
}
export function isValidAddressSegment(s: string): boolean {
  return ADDRESS_RE.test(s);
}
export function isValidSignatureFile(s: string): boolean {
  return SIG_FILE_RE.test(s);
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
const INDEX_TYPED_DATA_PREFIX = "/index/by-typed-data/";
const INDEX_BY_SELECTOR_PREFIX = "/index/by-selector/";
const TOKENS_PREFIX = "/tokens/";
const BUNDLES_PREFIX = "/bundles/";
const SIGNATURES_PREFIX = "/signatures/";
const CONTEXTS_PREFIX = "/contexts/";
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

  if (
    pathname.startsWith(INDEX_TYPED_DATA_PREFIX) &&
    pathname.endsWith(JSON_SUFFIX)
  ) {
    const seg = pathname.slice(
      INDEX_TYPED_DATA_PREFIX.length,
      pathname.length - JSON_SUFFIX.length,
    );
    return isTypedDataKey(seg)
      ? { ok: true, objectName: `index/by-typed-data/${seg}.json` }
      : { ok: false };
  }

  if (
    pathname.startsWith(INDEX_BY_SELECTOR_PREFIX) &&
    pathname.endsWith(JSON_SUFFIX)
  ) {
    const seg = pathname.slice(
      INDEX_BY_SELECTOR_PREFIX.length,
      pathname.length - JSON_SUFFIX.length,
    );
    return isValidSelectorKey(seg)
      ? { ok: true, objectName: `index/by-selector/${seg}.json` }
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

  if (pathname.startsWith(BUNDLES_PREFIX)) {
    const file = pathname.slice(BUNDLES_PREFIX.length);
    return BUNDLE_FILE_RE.test(file)
      ? { ok: true, objectName: `bundles/${file}` }
      : { ok: false };
  }

  // Detached bundle signatures — <bundle_sha256>.sig, served verbatim. The
  // extension fetches one per unique bundle (keyed by its recomputed hash) and
  // verifies it against the pinned key before installing the decoder.
  if (pathname.startsWith(SIGNATURES_PREFIX)) {
    const file = pathname.slice(SIGNATURES_PREFIX.length);
    return SIG_FILE_RE.test(file)
      ? { ok: true, objectName: `signatures/${file}` }
      : { ok: false };
  }

  if (pathname.startsWith(CONTEXTS_PREFIX) && pathname.endsWith(JSON_SUFFIX)) {
    const inner = pathname.slice(CONTEXTS_PREFIX.length);
    const parts = inner.split("/");
    if (parts.length < 4) return { ok: false };
    const file = parts[parts.length - 1];
    if (!file.endsWith(JSON_SUFFIX)) return { ok: false };
    const address = file.slice(0, file.length - JSON_SUFFIX.length);
    const chain = parts[parts.length - 2];
    const scopeParts = parts.slice(0, -2);
    const safe = scopeParts.every((part) => new RegExp(`^${SAFE_SEGMENT}$`).test(part));
    return safe &&
      isValidChainSegment(chain) &&
      isValidAddressSegment(address)
      ? { ok: true, objectName: `contexts/${inner}` }
      : { ok: false };
  }
  return { ok: false };
}
