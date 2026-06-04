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

/** Calls the SW `run-diagnosis-probes` op; returns the truth map id sets. */
export async function runDiagnosisProbes(
  input: DiagnosisRequestDto,
): Promise<DiagnosisResultDto> {
  return sendToExtension<DiagnosisResultDto>({
    type: "run-diagnosis-probes",
    input_json: JSON.stringify(input),
  });
}
