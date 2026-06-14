/**
 * bundle-verify.test.ts — the supply-chain verify gate. Uses REAL WebCrypto
 * (the same primitives the SW runs) with a freshly generated P-256 keypair, so
 * a green test means the actual verify path accepts genuine signatures and
 * fail-closes on every tamper / downgrade.
 */
import { beforeAll, describe, expect, it } from "vitest";
import canonicalize from "canonicalize";
import { verifyBundleSignature } from "../adapter-loader/bundle-verify";

const enc = new TextEncoder();

function b64(bytes: ArrayBuffer): string {
  const b = new Uint8Array(bytes);
  let s = "";
  for (let i = 0; i < b.length; i += 1) s += String.fromCharCode(b[i]);
  return btoa(s);
}

async function sha256Hex(s: string): Promise<string> {
  const d = await crypto.subtle.digest("SHA-256", enc.encode(s));
  const b = new Uint8Array(d);
  let h = "";
  for (let i = 0; i < b.length; i += 1) h += b[i].toString(16).padStart(2, "0");
  return "0x" + h;
}

const BASE = "https://registry.example";
const BUNDLE = { type: "adapter_action", id: "x@1", schema_version: "3", a: 1 };

let keyPair: CryptoKeyPair;
let pinnedSpkiB64: string;
let bundleSha: string;
let goodSigB64: string;

beforeAll(async () => {
  keyPair = (await crypto.subtle.generateKey(
    { name: "ECDSA", namedCurve: "P-256" },
    true,
    ["sign", "verify"],
  )) as CryptoKeyPair;
  pinnedSpkiB64 = b64(await crypto.subtle.exportKey("spki", keyPair.publicKey));
  const canonical = canonicalize(BUNDLE) as string;
  bundleSha = await sha256Hex(canonical);
  // WebCrypto sign emits P1363 (raw r||s) — the same format sign-bundles writes.
  const sig = await crypto.subtle.sign(
    { name: "ECDSA", hash: "SHA-256" },
    keyPair.privateKey,
    enc.encode(canonical),
  );
  goodSigB64 = b64(sig);
});

/** A fetch that serves `sigDoc` at /signatures/<expectSha>.sig, else 404. */
function sigFetch(expectSha: string, sigDoc: unknown): typeof fetch {
  return (async (input: RequestInfo | URL) => {
    const url = String(input);
    if (url === `${BASE}/signatures/${expectSha}.sig`) {
      return new Response(JSON.stringify(sigDoc), { status: 200 });
    }
    return new Response("not found", { status: 404 });
  }) as unknown as typeof fetch;
}

const sig = (b64s: string, alg = "ECDSA_P256_SHA256") => ({
  alg,
  key_id: "test",
  sig_b64: b64s,
});

describe("verifyBundleSignature", () => {
  it("require=false → no-op pass, no fetch", async () => {
    let fetched = false;
    const fetchImpl = (async () => {
      fetched = true;
      return new Response("", { status: 200 });
    }) as unknown as typeof fetch;
    const r = await verifyBundleSignature({
      bundle: BUNDLE,
      baseUrl: BASE,
      fetchImpl,
      require: false,
      pinnedKeySpkiB64: pinnedSpkiB64,
    });
    expect(r.ok).toBe(true);
    expect(fetched).toBe(false);
  });

  it("valid signature → ok", async () => {
    const r = await verifyBundleSignature({
      bundle: BUNDLE,
      claimedSha256: bundleSha,
      baseUrl: BASE,
      fetchImpl: sigFetch(bundleSha, sig(goodSigB64)),
      require: true,
      pinnedKeySpkiB64: pinnedSpkiB64,
    });
    expect(r).toMatchObject({ ok: true });
  });

  it("missing .sig (404) → fail-closed", async () => {
    const r = await verifyBundleSignature({
      bundle: BUNDLE,
      claimedSha256: bundleSha,
      baseUrl: BASE,
      fetchImpl: sigFetch("0xdeadbeef", sig(goodSigB64)), // wrong sha → 404 for ours
      require: true,
      pinnedKeySpkiB64: pinnedSpkiB64,
    });
    expect(r).toMatchObject({ ok: false, reason: "sig_http_404" });
  });

  it("sha mismatch (response lies about bundle_sha256) → fail-closed", async () => {
    const r = await verifyBundleSignature({
      bundle: BUNDLE,
      claimedSha256: "0x" + "f".repeat(64),
      baseUrl: BASE,
      fetchImpl: sigFetch(bundleSha, sig(goodSigB64)),
      require: true,
      pinnedKeySpkiB64: pinnedSpkiB64,
    });
    expect(r).toMatchObject({ ok: false, reason: "sha_mismatch" });
  });

  it("tampered bundle (genuine sig for different content) → fail-closed", async () => {
    const tampered = { ...BUNDLE, a: 999 };
    const tamperedSha = await sha256Hex(canonicalize(tampered) as string);
    // serve the GOOD sig (for BUNDLE) at the tampered bundle's sha URL
    const r = await verifyBundleSignature({
      bundle: tampered,
      claimedSha256: tamperedSha,
      baseUrl: BASE,
      fetchImpl: sigFetch(tamperedSha, sig(goodSigB64)),
      require: true,
      pinnedKeySpkiB64: pinnedSpkiB64,
    });
    expect(r).toMatchObject({ ok: false, reason: "sig_invalid" });
  });

  it("alg-confusion: a bogus .sig.alg is ignored when the sig is valid", async () => {
    const r = await verifyBundleSignature({
      bundle: BUNDLE,
      claimedSha256: bundleSha,
      baseUrl: BASE,
      fetchImpl: sigFetch(bundleSha, sig(goodSigB64, "none")),
      require: true,
      pinnedKeySpkiB64: pinnedSpkiB64,
    });
    expect(r).toMatchObject({ ok: true });
  });

  it("alg-confusion: a wrong sig still fails regardless of .sig.alg", async () => {
    const wrong = b64(new Uint8Array(64).fill(1).buffer);
    const r = await verifyBundleSignature({
      bundle: BUNDLE,
      claimedSha256: bundleSha,
      baseUrl: BASE,
      fetchImpl: sigFetch(bundleSha, sig(wrong, "none")),
      require: true,
      pinnedKeySpkiB64: pinnedSpkiB64,
    });
    expect(r).toMatchObject({ ok: false, reason: "sig_invalid" });
  });

  it("require=true but no pinned key → fail-closed", async () => {
    const r = await verifyBundleSignature({
      bundle: BUNDLE,
      claimedSha256: bundleSha,
      baseUrl: BASE,
      fetchImpl: sigFetch(bundleSha, sig(goodSigB64)),
      require: true,
      pinnedKeySpkiB64: "",
    });
    expect(r).toMatchObject({ ok: false, reason: "no_pinned_key" });
  });

  it("malformed .sig (no sig_b64) → fail-closed", async () => {
    const r = await verifyBundleSignature({
      bundle: BUNDLE,
      claimedSha256: bundleSha,
      baseUrl: BASE,
      fetchImpl: sigFetch(bundleSha, { alg: "x", key_id: "y" }),
      require: true,
      pinnedKeySpkiB64: pinnedSpkiB64,
    });
    expect(r).toMatchObject({ ok: false, reason: "sig_malformed" });
  });

  it("fetches the sig by the RECOMPUTED hash, not the (untrusted) claim", async () => {
    // claimedSha256 omitted entirely → still must fetch /signatures/<localSha>.sig
    const r = await verifyBundleSignature({
      bundle: BUNDLE,
      baseUrl: BASE,
      fetchImpl: sigFetch(bundleSha, sig(goodSigB64)),
      require: true,
      pinnedKeySpkiB64: pinnedSpkiB64,
    });
    expect(r).toMatchObject({ ok: true });
  });
});
