import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import {
  createExtensionClient,
  type Catalog,
  type ExtensionClient,
  type ManagedPolicy,
} from "@scopeball/sdk";
import {
  loadPreferences,
  subscribePreferences,
} from "./settings/preferences";

export type ConnectionStatus =
  | { kind: "connecting" }
  | { kind: "connected"; version: number }
  | { kind: "error"; message: string };

export interface ExtensionContextValue {
  client: ExtensionClient;
  catalog: Catalog | null;
  managed: ManagedPolicy[] | null;
  status: ConnectionStatus;
  refresh: () => Promise<void>;
}

// Exported so tests can mount a stand-in provider (see
// `testing/test-sdk-provider.tsx`) without going through the real
// connection-establishing `ExtensionProvider`.
export const ExtensionContext = createContext<ExtensionContextValue | null>(
  null,
);

// One client per app instance. `createExtensionClient()` registers a
// window.message listener — calling it once and reusing the instance is
// important to avoid double-handling responses.
export function ExtensionProvider({ children }: { children: ReactNode }) {
  const client = useMemo(() => createExtensionClient(), []);
  const [status, setStatus] = useState<ConnectionStatus>({ kind: "connecting" });
  const [catalog, setCatalog] = useState<Catalog | null>(null);
  const [managed, setManaged] = useState<ManagedPolicy[] | null>(null);

  const refresh = useCallback(async () => {
    try {
      const { version } = await client.ping();
      setStatus({ kind: "connected", version });
      const [c, m] = await Promise.all([
        client.getCatalog(),
        client.listManaged(),
      ]);
      setCatalog(c);
      setManaged(m);
    } catch (err) {
      setStatus({
        kind: "error",
        message: err instanceof Error ? err.message : String(err),
      });
    }
  }, [client]);

  // Stays in sync with Settings → "auto-refresh on change" toggle.
  // We re-read on every preference change so flipping the switch takes
  // effect immediately without re-mounting the provider.
  const autoRefreshRef = useRef<boolean>(loadPreferences().autoRefreshOnChange);
  useEffect(() => {
    return subscribePreferences(() => {
      autoRefreshRef.current = loadPreferences().autoRefreshOnChange;
    });
  }, []);

  useEffect(() => {
    void refresh();
    return client.onChange(() => {
      if (autoRefreshRef.current) void refresh();
    });
  }, [client, refresh]);

  const value = useMemo<ExtensionContextValue>(
    () => ({ client, catalog, managed, status, refresh }),
    [client, catalog, managed, status, refresh],
  );

  return (
    <ExtensionContext.Provider value={value}>
      {children}
    </ExtensionContext.Provider>
  );
}

export function useExtension(): ExtensionContextValue {
  const ctx = useContext(ExtensionContext);
  if (!ctx) {
    throw new Error(
      "useExtension must be used inside <ExtensionProvider>",
    );
  }
  return ctx;
}
