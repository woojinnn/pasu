/**
 * registry-api — private GCS bucket reader.
 *
 * proxy 는 PRIVATE 버킷 (Public Access Prevention enforced, allUsers binding
 * 없음) 에서 object 를 읽는다. 인증 없는 fetch 로는 불가. @google-cloud/storage
 * 가 ambient SA 의 OAuth access token 을 ADC 로 발급한다. Cloud Run 에선 ADC
 * 가 key 파일 없이 runtime SA 로 resolve 된다.
 *
 * ObjectReader 인터페이스 뒤에 둬서 HTTP 서버를 in-memory fake 로 unit-test
 * 가능 — 테스트는 실제 GCS / ADC 를 안 건드린다.
 */
import { Storage } from "@google-cloud/storage";

export interface ObjectFound {
  kind: "found";
  body: Buffer;
  contentType: string;
}
export interface ObjectNotFound {
  kind: "not_found";
}
export interface ObjectUpstreamError {
  kind: "upstream_error";
  message: string;
}
export type ObjectResult = ObjectFound | ObjectNotFound | ObjectUpstreamError;

export interface ObjectReader {
  read(objectName: string): Promise<ObjectResult>;
}

type GcsErrorClass = "not_found" | "upstream_error";

/**
 * throw 된 GCS error 분류. 404 (object 없음) 는 정상·예상 결과 — registry 에
 * 그 callkey entry 가 없다는 뜻. 그 외 (403=IAM misconfig, 5xx, network) 는
 * upstream fault.
 */
export function classifyGcsError(error: unknown): GcsErrorClass {
  const code =
    error && typeof error === "object" && "code" in error
      ? (error as { code: unknown }).code
      : undefined;
  if (code === 404 || code === "404") return "not_found";
  return "upstream_error";
}

export interface GcsObjectReaderOptions {
  bucketName: string;
  storage?: Storage; // 테스트 주입용
}

export class GcsObjectReader implements ObjectReader {
  private readonly storage: Storage;
  private readonly bucketName: string;

  constructor(o: GcsObjectReaderOptions) {
    this.storage = o.storage ?? new Storage();
    this.bucketName = o.bucketName;
  }

  async read(objectName: string): Promise<ObjectResult> {
    try {
      const file = this.storage.bucket(this.bucketName).file(objectName);
      const [buffer] = await file.download();
      return {
        kind: "found",
        body: buffer,
        // registry object 는 항상 JSON; builder 가 .json 만 쓴다.
        // 저장된 object metadata 는 신뢰하지 않는다.
        contentType: "application/json; charset=utf-8",
      };
    } catch (error) {
      if (classifyGcsError(error) === "not_found") return { kind: "not_found" };
      const message = error instanceof Error ? error.message : "GCS read failed";
      return { kind: "upstream_error", message };
    }
  }
}
