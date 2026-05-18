// RPC endpoint settings page — Phase 7.4.
//
// Route: `/rpc-endpoint`. The user sets / inspects the URL of the
// policy-rpc server the SW will call when evaluating manifests.
//
// SDK note: there is no dedicated `getEndpointUrl()` method. The SW's
// `manifest:ping` handler echoes back the configured URL in its result
// envelope (handlers.ts:99-122), so we seed the input from that. This
// is intentional — adding a new SW handler (which is off-limits in
// task 7.4) just to read the URL would be more code for no functional
// gain. A follow-up phase can promote it to its own SDK method if the
// ping-as-getter pattern becomes load-bearing in more places.
//
// "Clear all manifests" is rendered disabled in v1. The SW exposes
// `replaceAllManifests({})` internally but no message handler for the
// dashboard to invoke. Adding `manifest:clear-all` is out of scope for
// 7.4 (backend is reserved for 7.5's migration work). The button is
// kept (greyed out) so the affordance is discoverable.

import { useCallback, useEffect, useState } from "react";
import { useExtension } from "../sdk-context";
import "./rpc-endpoint.css";

type PingState =
  | { kind: "idle" }
  | { kind: "loading" }
  | {
      kind: "result";
      reachable: boolean;
      url: string | null;
      status?: number;
      message?: string;
    }
  | { kind: "error"; message: string };

function validateScheme(url: string): string | null {
  // Empty is "no URL configured" — allowed to save (clears storage).
  if (url.trim() === "") return null;
  if (/^https?:\/\//i.test(url)) return null;
  return "http:// or https:// only";
}

export function RpcEndpointPage(): JSX.Element {
  const { client } = useExtension();
  const [url, setUrl] = useState<string>("");
  const [scheme, setScheme] = useState<string | null>(null);
  const [save, setSave] = useState<
    { kind: "idle" } | { kind: "saving" } | { kind: "saved" } | { kind: "error"; message: string }
  >({ kind: "idle" });
  const [ping, setPing] = useState<PingState>({ kind: "idle" });

  // Seed the input from the SW (see file header for why we use ping
  // here rather than a dedicated getter).
  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const r = await client.pingRpcEndpoint();
        if (!cancelled && r.url) setUrl(r.url);
      } catch {
        // Mount-time errors are non-fatal — the user can still type a
        // URL and Save. The explicit Ping button surfaces errors later.
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [client]);

  const onSave = useCallback(async () => {
    const err = validateScheme(url);
    setScheme(err);
    if (err) return;
    setSave({ kind: "saving" });
    try {
      const next = url.trim() === "" ? null : url.trim();
      await client.setEndpointUrl(next);
      setSave({ kind: "saved" });
    } catch (e) {
      setSave({
        kind: "error",
        message: e instanceof Error ? e.message : String(e),
      });
    }
  }, [client, url]);

  const onPing = useCallback(async () => {
    setPing({ kind: "loading" });
    try {
      const r = await client.pingRpcEndpoint();
      setPing({ kind: "result", ...r });
    } catch (e) {
      setPing({
        kind: "error",
        message: e instanceof Error ? e.message : String(e),
      });
    }
  }, [client]);

  return (
    <div className="rpc-endpoint">
      <header>
        <h1>policy-rpc endpoint</h1>
        <p>
          The URL the engine calls to evaluate per-action manifests.
          Manifests are stored per endpoint — pointing here at a
          different server resets which custom fields your policies can
          reference.
        </p>
      </header>

      <section className="rpc-endpoint-field">
        <label htmlFor="rpc-endpoint-url">Endpoint URL</label>
        <input
          id="rpc-endpoint-url"
          type="text"
          value={url}
          placeholder="https://policy-rpc.example.com"
          onChange={(e) => {
            setUrl(e.target.value);
            setScheme(null);
            setSave({ kind: "idle" });
          }}
        />
        {scheme !== null ? (
          <p className="rpc-endpoint-error" role="alert">
            {scheme}
          </p>
        ) : null}
        {save.kind === "saved" ? (
          <p className="rpc-endpoint-ok">Saved.</p>
        ) : null}
        {save.kind === "error" ? (
          <p className="rpc-endpoint-error" role="alert">
            {save.message}
          </p>
        ) : null}
      </section>

      <section className="rpc-endpoint-actions">
        <button type="button" onClick={onSave} disabled={save.kind === "saving"}>
          Save
        </button>
        <button type="button" onClick={onPing} disabled={ping.kind === "loading"}>
          Ping
        </button>
      </section>

      {ping.kind === "result" ? (
        <section
          className={
            ping.reachable ? "rpc-ping-ok" : "rpc-ping-fail"
          }
          data-testid="ping-result"
        >
          <strong>{ping.reachable ? "reachable" : "unreachable"}</strong>
          {ping.url ? <> at <code>{ping.url}</code></> : null}
          {typeof ping.status === "number" ? <> — HTTP {ping.status}</> : null}
          {ping.message ? <> — {ping.message}</> : null}
        </section>
      ) : null}
      {ping.kind === "error" ? (
        <section className="rpc-ping-fail">
          <strong>error:</strong> {ping.message}
        </section>
      ) : null}

      <section className="rpc-endpoint-danger">
        <button
          type="button"
          disabled
          title="Coming in v2 — backend handler is not yet wired"
        >
          Clear all manifests
        </button>
        <p className="rpc-endpoint-hint">
          Deferred. Track in follow-up: a SW <code>manifest:clear-all</code>{" "}
          handler + SDK method are needed first.
        </p>
      </section>
    </div>
  );
}
