import Browser from "webextension-polyfill";
import { Identifier } from "@lib/identifier";
import { sendToPortAndDisregard } from "@lib/messages";
import { RequestType } from "@lib/types";

let metamaskChainId = 1;

function checkMethod(item: any, method: string): boolean {
  return String(item?.method).toLowerCase().includes(method.toLowerCase());
}

function forwardBypassed(data: any): void {
  const port = Browser.runtime.connect({ name: Identifier.CONTENT_SCRIPT });
  sendToPortAndDisregard(port, data);
}

function checkMetaMaskBypass(messageData: any): void {
  const items = Array.isArray(messageData) ? messageData : [messageData];
  const hostname = location.hostname;
  for (const item of items) {
    if (!item) continue;
    if (checkMethod(item, "eth_sendTransaction")) {
      const [transaction] = item.params ?? [];
      forwardBypassed({
        type: RequestType.TRANSACTION,
        bypassed: true,
        hostname,
        chainId: metamaskChainId,
        transaction,
      });
    } else if (checkMethod(item, "eth_signTypedData")) {
      const [address, typedDataStr] = item.params ?? [];
      try {
        const typedData =
          typeof typedDataStr === "string"
            ? JSON.parse(typedDataStr)
            : typedDataStr;
        forwardBypassed({
          type: RequestType.TYPED_SIGNATURE,
          bypassed: true,
          hostname,
          chainId: metamaskChainId,
          address,
          typedData,
        });
      } catch {
        /* ignore malformed typed data */
      }
    } else if (
      checkMethod(item, "eth_sign") ||
      checkMethod(item, "personal_sign")
    ) {
      const [first, second] = item.params ?? [];
      const message =
        String(first).replace(/^0x/, "").length === 40 ? second : first;
      forwardBypassed({
        type: RequestType.UNTYPED_SIGNATURE,
        bypassed: true,
        hostname,
        message: String(message ?? ""),
      });
    }
    // wallet_sendCalls (EIP-5792) is intentionally NOT observed in v1.
    // Plan 5/6 may revisit batch-evaluation semantics; until then, treating
    // each call as a bypassed transaction would surface confusing
    // half-evaluated state to the SW.
  }
}

window.addEventListener("message", (event) => {
  const target = event?.data?.target;
  const inner = event?.data?.data;
  if (!inner) return;
  if (inner.name === Identifier.METAMASK_PROVIDER) {
    if (target === Identifier.METAMASK_CONTENT_SCRIPT)
      checkMetaMaskBypass(inner.data);
    if (
      target === Identifier.METAMASK_INPAGE &&
      inner.data?.method?.includes("chainChanged")
    ) {
      metamaskChainId = Number(inner.data?.params?.chainId ?? metamaskChainId);
    }
  }
});

window.addEventListener("message", (event) => {
  const { type, data } = event?.data ?? {};
  if (type !== Identifier.COINBASE_WALLET_REQUEST || !data) return;
  const hostname = location.hostname;
  if (data.request?.method === "signEthereumTransaction") {
    forwardBypassed({
      type: RequestType.TRANSACTION,
      bypassed: true,
      hostname,
      chainId: Number(data.request.params.chainId ?? 1),
      transaction: {
        from: data.request.params.fromAddress,
        to: data.request.params.toAddress,
        data: data.request.params.data,
        value: Number.parseInt(data.request.params.weiValue ?? "0").toString(
          16,
        ),
      },
    });
  } else if (data.request?.method === "signEthereumMessage") {
    const typedDataStr = data.request.params.typedDataJson;
    if (typedDataStr) {
      try {
        const typedData = JSON.parse(typedDataStr);
        forwardBypassed({
          type: RequestType.TYPED_SIGNATURE,
          bypassed: true,
          hostname,
          chainId: Number(typedData?.domain?.chainId ?? 1),
          address: data.request.params.address,
          typedData,
        });
      } catch {
        /* ignore */
      }
    } else {
      forwardBypassed({
        type: RequestType.UNTYPED_SIGNATURE,
        bypassed: true,
        hostname,
        message: String(data.request.params.message ?? ""),
      });
    }
  }
});
