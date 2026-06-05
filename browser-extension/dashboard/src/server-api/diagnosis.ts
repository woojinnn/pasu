/**
 * Bridge call into the WASM denial-diagnosis oracle. This is the one network-ish
 * hop in the flow: dashboard → service worker (`run-diagnosis-probes` op) → WASM
 * `run_diagnosis_probes_v2_json`. See `cedar/diagnosis/README.md`.
 *
 * Build the probes with `buildProbes(policy)`, run them via `runDiagnosisProbes(...)`
 * (this function), then pass the policy, the probe ids, and the returned result into
 * `diagnoseFromResult`. The `action`/`meta`/`tx`/`bundles`/`results` describe the
 * transaction context the policy is evaluated against (use `SAMPLE_ACTIONS` for the
 * editor; the live verdict's own context for the popup).
 */

import { sendToExtension } from "./extension-bridge";

/** One probe sent to the WASM diagnosis runner. */
export interface ProbeDto {
  /** Structural node path, doubles as the probe @id. */
  id: string;
  /** EST of `permit(...) when { <subtree> }` produced by blocksToEst. */
  est: unknown;
}

export interface DiagnosisRequestDto {
  action: unknown;
  meta: unknown;
  tx: { chain_id: string; from: string; to: string };
  bundles: { policy: string; manifest: unknown }[];
  results: Record<string, unknown>;
  probes: ProbeDto[];
}

export interface DiagnosisResultDto {
  true_ids: string[];
  error_ids: string[];
}

/** Calls the SW `run-diagnosis-probes` op; returns the truth map id sets
 *  (`{ true_ids, error_ids }`).
 *
 *  There are TWO envelope layers to peel: the WASM export returns a JSON STRING
 *  `{ ok, data }`, and `sendToExtension` resolves the service-worker wrapper to
 *  that raw string. So we `JSON.parse` it here and unwrap `data` / throw on
 *  `error`. (Forgetting this parse is exactly the bug fixed in 43ecc32a — it left
 *  `result.true_ids` undefined → an empty truth map → silent no-highlight.) */
export async function runDiagnosisProbes(
  input: DiagnosisRequestDto,
): Promise<DiagnosisResultDto> {
  const raw = await sendToExtension<string>({
    type: "run-diagnosis-probes",
    input_json: JSON.stringify(input),
  });
  const envelope = JSON.parse(raw) as
    | { ok: true; data: DiagnosisResultDto }
    | { ok: false; error: { kind: string; message: string } };
  if (!envelope.ok) {
    throw new Error(envelope.error.message);
  }
  return envelope.data;
}
