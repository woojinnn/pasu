// First-run onboarding wizard — Phase 7.4.
//
// Route: `/onboarding`. Shown to new users in production when both
// `rpc:endpointUrl` is null and `rpc:manifests` is empty. The default
// landing redirect (App → /onboarding) is deferred to a follow-up; for
// now reach this page from settings or by direct navigation.
//
// Three steps, in order:
//   1. Intro — explains the per-RPC-endpoint model in two sentences.
//   2. Endpoint URL — same scheme validation as `/rpc-endpoint`. A
//      "Use local dev server" shortcut is rendered only when
//      `process.env.NODE_ENV !== "production"` so production builds
//      don't surface a dead URL to end users.
//   3. Import manifests — paste-JSON of `{[action]: PolicyManifest}`
//      or "Skip" to land on `/policies`. We POST one manifest at a
//      time so partial failures don't abort the rest.

import { useCallback, useState } from "react";
import { useNavigate } from "react-router-dom";
import type { PolicyManifest } from "@scopeball/sdk";
import { useExtension } from "../sdk-context";
import "./onboarding.css";

const LOCAL_DEV_URL = "http://localhost:8787";

function isDevBuild(): boolean {
  // Vite inlines `process.env.NODE_ENV` at build time. Vitest sets
  // `process.env.NODE_ENV === "test"`, so the shortcut is visible
  // under test (and dev), and invisible only when the production
  // bundle is served. We avoid optional chaining (`process.env?.NODE_ENV`)
  // because Vite's documented contract is the literal text
  // `process.env.NODE_ENV`; the optional-chain form may not be
  // statically replaced.
  return (
    typeof process !== "undefined" &&
    process.env.NODE_ENV !== "production"
  );
}

function validateScheme(url: string): string | null {
  if (url.trim() === "") return "Endpoint URL is required";
  if (/^https?:\/\//i.test(url)) return null;
  return "http:// or https:// only";
}

interface ImportError {
  action: string;
  kind: string;
  message: string;
}

export function OnboardingPage(): JSX.Element {
  const { client } = useExtension();
  const navigate = useNavigate();

  const [step, setStep] = useState<1 | 2 | 3>(1);

  // Step 2 state.
  const [url, setUrl] = useState<string>("");
  const [urlErr, setUrlErr] = useState<string | null>(null);

  // Step 3 state.
  const [paste, setPaste] = useState<string>("");
  const [pasteErr, setPasteErr] = useState<string | null>(null);
  const [errors, setErrors] = useState<ImportError[]>([]);
  const [importing, setImporting] = useState<boolean>(false);

  const onSaveUrlAndNext = useCallback(async () => {
    const err = validateScheme(url);
    setUrlErr(err);
    if (err) return;
    try {
      await client.setEndpointUrl(url.trim());
      setStep(3);
    } catch (e) {
      setUrlErr(e instanceof Error ? e.message : String(e));
    }
  }, [client, url]);

  const onImport = useCallback(async () => {
    setPasteErr(null);
    setErrors([]);

    let parsed: unknown;
    try {
      parsed = JSON.parse(paste);
    } catch (e) {
      setPasteErr(
        `invalid JSON: ${e instanceof Error ? e.message : String(e)}`,
      );
      return;
    }

    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
      setPasteErr("invalid JSON: expected an object map of action → manifest");
      return;
    }

    const entries = Object.entries(parsed as Record<string, PolicyManifest>);
    setImporting(true);
    const failures: ImportError[] = [];
    for (const [action, manifest] of entries) {
      try {
        await client.putManifest(action, manifest);
      } catch (e) {
        const annotated = e as {
          kind?: unknown;
          message?: unknown;
        };
        failures.push({
          action,
          kind:
            typeof annotated.kind === "string"
              ? (annotated.kind as string)
              : "manifest_failed",
          message:
            typeof annotated.message === "string"
              ? (annotated.message as string)
              : String(e),
        });
      }
    }
    setImporting(false);
    if (failures.length > 0) {
      setErrors(failures);
      // Don't navigate — let the user fix and retry.
      return;
    }
    navigate("/policies");
  }, [client, navigate, paste]);

  const onSkip = useCallback(() => {
    navigate("/policies");
  }, [navigate]);

  return (
    <div className="onboarding">
      <header className="onboarding-head">
        <h1>Set up scopeball</h1>
        <p className="onboarding-progress">Step {step} of 3</p>
      </header>

      {step === 1 ? (
        <section className="onboarding-step">
          <h2>Set your policy-rpc endpoint</h2>
          <p>
            scopeball evaluates policies against a per-action manifest that
            describes the off-chain data each policy reads. Manifests are
            stored per <code>policy-rpc</code> endpoint, so each endpoint
            you point the extension at carries its own set of custom
            context fields.
          </p>
          <p>
            We'll walk you through pointing at a URL and (optionally)
            importing a starter set of manifests.
          </p>
          <div className="onboarding-actions">
            <button type="button" onClick={() => setStep(2)}>
              Next
            </button>
          </div>
        </section>
      ) : null}

      {step === 2 ? (
        <section className="onboarding-step">
          <h2>Endpoint URL</h2>
          <label htmlFor="onboarding-url">Endpoint URL</label>
          <input
            id="onboarding-url"
            type="text"
            value={url}
            placeholder="https://policy-rpc.example.com"
            onChange={(e) => {
              setUrl(e.target.value);
              setUrlErr(null);
            }}
          />
          {urlErr ? (
            <p className="onboarding-error" role="alert">
              {urlErr}
            </p>
          ) : null}
          {isDevBuild() ? (
            <button
              type="button"
              className="onboarding-shortcut"
              onClick={() => {
                setUrl(LOCAL_DEV_URL);
                setUrlErr(null);
              }}
            >
              Use local dev server ({LOCAL_DEV_URL})
            </button>
          ) : null}
          <div className="onboarding-actions">
            <button type="button" onClick={() => setStep(1)}>
              Back
            </button>
            <button type="button" onClick={onSaveUrlAndNext}>
              Next
            </button>
          </div>
        </section>
      ) : null}

      {step === 3 ? (
        <section className="onboarding-step">
          <h2>Import manifests</h2>
          <p>
            Paste a JSON object mapping action names to{" "}
            <code>PolicyManifest</code> objects, or skip to start with no
            manifests installed.
          </p>
          <label htmlFor="onboarding-paste">Paste manifests JSON</label>
          <textarea
            id="onboarding-paste"
            rows={10}
            value={paste}
            placeholder='{"swap": { "id": "...", "schema_version": 1, "requires": [] }}'
            onChange={(e) => {
              setPaste(e.target.value);
              setPasteErr(null);
              setErrors([]);
            }}
          />
          {pasteErr ? (
            <p className="onboarding-error" role="alert">
              {pasteErr}
            </p>
          ) : null}
          {errors.length > 0 ? (
            <ul className="onboarding-error-list">
              {errors.map((err) => (
                <li key={err.action}>
                  <strong>{err.action}</strong>: <code>{err.kind}</code> —{" "}
                  {err.message}
                </li>
              ))}
            </ul>
          ) : null}
          <div className="onboarding-actions">
            <button type="button" onClick={() => setStep(2)}>
              Back
            </button>
            <button type="button" onClick={onSkip}>
              Skip
            </button>
            <button type="button" onClick={onImport} disabled={importing}>
              Import
            </button>
          </div>
        </section>
      ) : null}
    </div>
  );
}
