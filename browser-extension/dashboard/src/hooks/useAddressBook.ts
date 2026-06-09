/**
 * useAddressBook — resolve raw 0x addresses to friendly names.
 *
 * Merges two per-user sources into one address→name map (+ a suggestion list
 * for autocomplete):
 *   - my wallets   (getDashboardSummary().wallets → label)   "내 지갑"
 *   - held tokens  (listTokens → symbol)                     "토큰"
 *
 * Wallet labels win over token symbols on the same address. Fails soft: if the
 * server/extension is unavailable the map is empty and callers fall back to the
 * raw address.
 */

import { useMemo } from "react";
import { useQuery } from "@tanstack/react-query";

import { getDashboardSummary, listTokens, tokenAddress } from "../server-api";
import { KNOWN_TOKENS } from "./known-tokens.generated";

export type AddressKind = "wallet" | "token";

export interface AddressEntry {
  /** Lowercased 0x address. */
  address: string;
  /** Friendly name (wallet label or token symbol). */
  name: string;
  kind: AddressKind;
  /** Secondary tag shown under the name in pickers ("내 지갑" / "토큰"). */
  sub: string;
}

export interface AddressBook {
  /** Resolve one address (case-insensitive) to its entry, or undefined. */
  lookup: (address: string) => AddressEntry | undefined;
  /** All known entries (wallets first), for autocomplete suggestions. */
  suggestions: AddressEntry[];
  loading: boolean;
}

/** Short form `0x1234…abcd` for display next to a resolved name. */
export function shortAddress(address: string): string {
  const a = address.trim();
  return a.length > 12 ? `${a.slice(0, 6)}…${a.slice(-4)}` : a;
}

export function useAddressBook(): AddressBook {
  const walletsQ = useQuery({
    queryKey: ["dashboard", "summary"],
    queryFn: getDashboardSummary,
    staleTime: 60_000,
    retry: false,
  });
  const tokensQ = useQuery({
    queryKey: ["tokens"],
    queryFn: listTokens,
    staleTime: 5 * 60_000,
    retry: false,
  });

  return useMemo<AddressBook>(() => {
    const map = new Map<string, AddressEntry>();
    const suggestions: AddressEntry[] = [];

    // Wallets first — their labels take priority over token symbols.
    for (const w of walletsQ.data?.wallets ?? []) {
      const label = w.label?.trim();
      if (!label) continue;
      const address = w.address.toLowerCase();
      if (map.has(address)) continue;
      const entry: AddressEntry = { address, name: label, kind: "wallet", sub: "내 지갑" };
      map.set(address, entry);
      suggestions.push(entry);
    }
    for (const t of tokensQ.data ?? []) {
      const address = tokenAddress(t.key);
      const symbol = t.symbol?.trim();
      if (!address || !symbol || map.has(address)) continue;
      const entry: AddressEntry = { address, name: symbol, kind: "token", sub: "토큰" };
      map.set(address, entry);
      suggestions.push(entry);
    }
    // Bundled common tokens — resolve well-known addresses even when not held.
    for (const [address, symbol] of KNOWN_TOKENS) {
      if (map.has(address)) continue;
      const entry: AddressEntry = { address, name: symbol, kind: "token", sub: "토큰" };
      map.set(address, entry);
      suggestions.push(entry);
    }

    return {
      lookup: (address: string) => map.get(address.trim().toLowerCase()),
      suggestions,
      loading: walletsQ.isLoading || tokensQ.isLoading,
    };
  }, [walletsQ.data, tokensQ.data, walletsQ.isLoading, tokensQ.isLoading]);
}
