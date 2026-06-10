/**
 * policy-store — 정책 스토리지 v2의 유일한 읽기/쓰기 게이트.
 * 모든 변경은 mutate()의 직렬 큐를 지나 불변식 검증 후 멀티키 atomic set으로
 * 커밋된다. 대시보드/popup은 SW 메시지로만 접근하므로 멀티탭 race가 없다.
 */
import Browser from "webextension-polyfill";

import { UNCATEGORIZED_PKG, type LibraryDoc, type StoreSnapshot, type WalletsDoc } from "./types";

const libKey = (uid: string) => `ps2:${uid}:library`;
const walKey = (uid: string) => `ps2:${uid}:wallets`;
const revKey = (uid: string) => `ps2:${uid}:rev`;

function emptyLibrary(): LibraryDoc {
  return {
    schemaVersion: 1,
    defs: {},
    packages: {
      [UNCATEGORIZED_PKG]: {
        id: UNCATEGORIZED_PKG,
        displayName: "미분류",
        source: "builtin",
        updatedAtMs: 0,
      },
    },
  };
}
const emptyWallets = (): WalletsDoc => ({ schemaVersion: 1, byAddress: {} });

export async function readStore(uid: string): Promise<StoreSnapshot> {
  const got = (await Browser.storage.local.get([libKey(uid), walKey(uid), revKey(uid)])) as Record<string, unknown>;
  return {
    library: (got[libKey(uid)] as LibraryDoc | undefined) ?? emptyLibrary(),
    wallets: (got[walKey(uid)] as WalletsDoc | undefined) ?? emptyWallets(),
    rev: (got[revKey(uid)] as number | undefined) ?? 0,
  };
}

/** 불변식: 바인딩의 defId/packageId 실재, 미분류 패키지 존재, 소문자 주소 키. */
function validate(s: StoreSnapshot): void {
  if (!s.library.packages[UNCATEGORIZED_PKG]) {
    throw new Error("미분류 패키지(pkg::uncategorized)는 삭제할 수 없습니다");
  }
  for (const [addr, w] of Object.entries(s.wallets.byAddress)) {
    if (addr !== addr.toLowerCase()) throw new Error(`지갑 주소는 소문자여야 합니다: ${addr}`);
    for (const b of Object.values(w.bindings)) {
      if (!s.library.defs[b.defId]) throw new Error(`바인딩 ${b.id}의 defId가 라이브러리에 없습니다: ${b.defId}`);
      if (!s.library.packages[b.packageId]) {
        throw new Error(`바인딩 ${b.id}의 packageId가 라이브러리에 없습니다: ${b.packageId}`);
      }
    }
  }
}

// 단일 직렬 큐 — 모든 계정의 쓰기가 순서대로 커밋된다.
let chain: Promise<unknown> = Promise.resolve();

export function mutate<T>(uid: string, fn: (draft: StoreSnapshot) => T | Promise<T>): Promise<T> {
  const run = chain.then(async () => {
    const current = await readStore(uid);
    const draft = structuredClone(current);
    const out = await fn(draft);
    validate(draft);
    draft.rev = current.rev + 1;
    await Browser.storage.local.set({
      [libKey(uid)]: draft.library,
      [walKey(uid)]: draft.wallets,
      [revKey(uid)]: draft.rev,
    });
    return out;
  });
  chain = run.catch(() => undefined); // 실패해도 큐는 계속
  return run;
}
