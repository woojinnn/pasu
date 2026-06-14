const { merge } = require("webpack-merge");
process.env.NODE_ENV = process.env.NODE_ENV || "production";
const common = require("./webpack.common.js");

// Audit Round 7+ (P1) — production builds must point at a real HTTPS
// registry. A missing or `http://localhost:*` `REGISTRY_BASE_URL` in a
// distributed extension would silently fall back to the dev server, which
// is both unreachable from the user's browser and a vector for downgrade
// attacks against the bundle / token registry trust path. We fail the
// build instead of letting that ship.
const registryBaseUrl = process.env.REGISTRY_BASE_URL ?? "";
if (process.env.DAMBI_ALLOW_INSECURE_REGISTRY !== "1") {
  if (!registryBaseUrl) {
    throw new Error(
      "[webpack.prod] REGISTRY_BASE_URL must be set for production builds. " +
        "Set it in browser-extension/.env (e.g. https://storage.googleapis.com/...) " +
        "or export DAMBI_ALLOW_INSECURE_REGISTRY=1 to bypass for a local " +
        "smoke test build.",
    );
  }
  if (!/^https:\/\//i.test(registryBaseUrl)) {
    throw new Error(
      `[webpack.prod] REGISTRY_BASE_URL must be https:// (got ${JSON.stringify(
        registryBaseUrl,
      )}). Override with DAMBI_ALLOW_INSECURE_REGISTRY=1 only for local smoke tests.`,
    );
  }
}

// Bundle-signature pinned-key guard. If this build ENFORCES signatures, a
// pinned public key MUST be present — otherwise every bundle install would
// fail-closed (warn) and the extension would decode nothing. Fail the build.
if (
  process.env.DAMBI_REQUIRE_BUNDLE_SIGNATURE === "true" &&
  !(process.env.PINNED_BUNDLE_PUBLIC_KEY ?? "").trim()
) {
  throw new Error(
    "[webpack.prod] DAMBI_REQUIRE_BUNDLE_SIGNATURE=true but PINNED_BUNDLE_PUBLIC_KEY " +
      "is empty. Set the SPKI(base64) pinned signing key in browser-extension/.env " +
      "(from `gcloud kms keys versions get-public-key` or `npm run gen-signing-key`), " +
      "or unset the require flag for an unsigned-registry build.",
  );
}

const prodOverrides = {
  mode: "production",
  devtool: false,
  optimization: { minimize: true },
};

module.exports = common.map((cfg) => merge(cfg, prodOverrides));
