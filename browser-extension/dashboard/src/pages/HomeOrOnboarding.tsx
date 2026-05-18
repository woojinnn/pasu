// D11 cold-start onboarding gate (Phase 7 carry-over I).
//
// Spec: "redirect to /onboarding when both `rpc:endpointUrl` is null
// AND `rpc:manifests` is empty". The router previously dropped fresh
// installs straight on `/` (HomePage), which links the user to
// `/editor` and `/library` — surfaces that assume a working policy-rpc
// endpoint and at least one manifest. New users would hit blank cards
// or unhelpful errors before discovering the existence of `/onboarding`.
//
// Implementation: this component is mounted at the index route. It
// reads the configured endpoint URL (`pingRpcEndpoint().url`) and the
// installed enriched schema (`getEnrichedSchema().customContexts`).
// When both are empty → `<Navigate to="/onboarding" replace>`. Otherwise
// renders `<HomePage>` unchanged. Loading state shows a tiny
// `<div aria-busy>` placeholder so the page doesn't flash HomePage
// before the redirect resolves.

import { useEffect, useState } from "react";
import { Navigate } from "react-router-dom";
import { HomePage } from "./HomePage";
import { useExtension } from "../sdk-context";

type Decision =
  | { kind: "loading" }
  | { kind: "home" }
  | { kind: "onboarding" };

export function HomeOrOnboarding(): JSX.Element {
  const { client } = useExtension();
  const [decision, setDecision] = useState<Decision>({ kind: "loading" });

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const [{ url }, schema] = await Promise.all([
          client.pingRpcEndpoint(),
          // `getEnrichedSchema()` is the cheapest way to inspect whether
          // any manifest has been installed — `customContexts` is the
          // per-action custom-field map. If it's empty for every action
          // the user has no manifests in the engine.
          client.getEnrichedSchema(),
        ]);
        if (cancelled) return;
        const customContexts = (schema?.customContexts ?? {}) as Record<
          string,
          unknown[]
        >;
        const hasAnyManifest = Object.values(customContexts).some(
          (rows) => Array.isArray(rows) && rows.length > 0,
        );
        const noEndpoint = url === null || url === undefined || url === "";
        if (noEndpoint && !hasAnyManifest) {
          setDecision({ kind: "onboarding" });
        } else {
          setDecision({ kind: "home" });
        }
      } catch {
        // On any read failure, fall back to HomePage. The user can
        // still navigate to /onboarding manually from settings.
        if (!cancelled) setDecision({ kind: "home" });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [client]);

  if (decision.kind === "loading") {
    return (
      <div className="home-or-onboarding-loading" aria-busy="true" />
    );
  }
  if (decision.kind === "onboarding") {
    return <Navigate to="/onboarding" replace />;
  }
  return <HomePage />;
}
