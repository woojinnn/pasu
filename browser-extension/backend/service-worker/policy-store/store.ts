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
  const snap: StoreSnapshot = {
    library: (got[libKey(uid)] as LibraryDoc | undefined) ?? emptyLibrary(),
    wallets: (got[walKey(uid)] as WalletsDoc | undefined) ?? emptyWallets(),
    rev: (got[revKey(uid)] as number | undefined) ?? 0,
  };
  normalizeWallets(snap);
  return snap;
}

/** 불변식: 바인딩의 defId 실재 + packageId는 그 지갑의 패키지(또는 미분류),
 *  미분류 라이브러리 폴더 존재, 소문자 주소 키. */
function validate(s: StoreSnapshot): void {
  if (!s.library.packages[UNCATEGORIZED_PKG]) {
    throw new Error("미분류 패키지(pkg::uncategorized)는 삭제할 수 없습니다");
  }
  for (const [addr, w] of Object.entries(s.wallets.byAddress)) {
    if (addr !== addr.toLowerCase()) throw new Error(`지갑 주소는 소문자여야 합니다: ${addr}`);
    for (const b of Object.values(w.bindings)) {
      if (!s.library.defs[b.defId]) throw new Error(`바인딩 ${b.id}의 defId가 라이브러리에 없습니다: ${b.defId}`);
      if (b.packageId !== UNCATEGORIZED_PKG && !w.packages[b.packageId]) {
        throw new Error(`바인딩 ${b.id}의 packageId가 지갑에 없습니다: ${b.packageId}`);
      }
    }
  }
}

/** 구 스토어 마이그레이션(읽기 시 정규화): 지갑 패키지 분리 이전에는 바인딩이
 *  계정(라이브러리) 패키지를 가리켰다 — 같은 id의 지갑 패키지를 이름을 복사해
 *  실체화한다. 다음 mutate 때 자연히 영속화된다. */
function normalizeWallets(s: StoreSnapshot): void {
  for (const w of Object.values(s.wallets.byAddress)) {
    w.packages ??= {};
    for (const b of Object.values(w.bindings)) {
      if (b.packageId === UNCATEGORIZED_PKG || w.packages[b.packageId]) continue;
      w.packages[b.packageId] = {
        id: b.packageId,
        displayName: s.library.packages[b.packageId]?.displayName ?? b.packageId,
        updatedAtMs: 0,
      };
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
