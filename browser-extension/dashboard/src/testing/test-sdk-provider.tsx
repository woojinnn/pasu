// Test-only SDK provider — mounts a caller-supplied `ExtensionClient`
// into the same `ExtensionContext` that the production
// `ExtensionProvider` uses. The real provider opens a `window.message`
// channel to the extension service worker, which has no analogue under
// happy-dom; this lightweight stand-in lets page tests render with a
// mock client and a connected status without any side effects.
//
// Lives alongside `sdk-context.tsx` so the page modules don't have to
// distinguish dev/test imports.

import type { ReactNode } from "react";
import type {
  Catalog,
  ExtensionClient,
  ManagedPolicy,
} from "@scopeball/sdk";
import {
  ExtensionContext,
  type ExtensionContextValue,
  type ConnectionStatus,
} from "../sdk-context";

export interface TestSdkProviderProps {
  client: ExtensionClient;
  /** Overrides for the context fields. Default: status=connected, no data. */
  catalog?: Catalog | null;
  managed?: ManagedPolicy[] | null;
  status?: ConnectionStatus;
  refresh?: () => Promise<void>;
  children: ReactNode;
}

export function TestSdkProvider(props: TestSdkProviderProps): JSX.Element {
  const value: ExtensionContextValue = {
    client: props.client,
    catalog: props.catalog ?? null,
    managed: props.managed ?? null,
    status: props.status ?? { kind: "connected", version: 1 },
    refresh: props.refresh ?? (async () => {}),
  };
  return (
    <ExtensionContext.Provider value={value}>
      {props.children}
    </ExtensionContext.Provider>
  );
}
