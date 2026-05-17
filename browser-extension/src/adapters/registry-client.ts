import type { Manifest, ChainId, Hex } from "./types";

export interface ChainResolution {
  manifest: Manifest;
  wasm_url: string;
  version: string;
  sdk_version: number;
}

export class RegistryClient {
  constructor(
    private baseUrl: string,
    private fetchFn: typeof fetch = fetch
  ) {}

  async resolve(chainId: ChainId, address: Hex): Promise<ChainResolution | null> {
    const url = `${this.baseUrl}/chains/${chainId}/${address.toLowerCase()}`;
    const res = await this.fetchFn(url);
    if (res.status === 404) return null;
    if (!res.ok) {
      throw new Error(`registry resolve ${chainId}/${address}: ${res.status}`);
    }
    return (await res.json()) as ChainResolution;
  }

  async fetchWasm(path: string): Promise<Uint8Array> {
    const url = path.startsWith("http") ? path : `${this.baseUrl}${path}`;
    const res = await this.fetchFn(url);
    if (!res.ok) throw new Error(`fetch wasm ${url}: ${res.status}`);
    return new Uint8Array(await res.arrayBuffer());
  }
}
