const { merge } = require("webpack-merge");
const common = require("./webpack.common.js");

// Audit Round 7+ (P1) — production builds must point at a real HTTPS
// registry. A missing or `http://localhost:*` `REGISTRY_BASE_URL` in a
// distributed extension would silently fall back to the dev server, which
// is both unreachable from the user's browser and a vector for downgrade
// attacks against the bundle / token registry trust path. We fail the
// build instead of letting that ship.
const registryBaseUrl = process.env.REGISTRY_BASE_URL ?? "";
if (process.env.SCOPEBALL_ALLOW_INSECURE_REGISTRY !== "1") {
  if (!registryBaseUrl) {
    throw new Error(
      "[webpack.prod] REGISTRY_BASE_URL must be set for production builds. " +
        "Set it in browser-extension/.env (e.g. https://storage.googleapis.com/...) " +
        "or export SCOPEBALL_ALLOW_INSECURE_REGISTRY=1 to bypass for a local " +
        "smoke test build.",
    );
  }
  if (!/^https:\/\//i.test(registryBaseUrl)) {
    throw new Error(
      `[webpack.prod] REGISTRY_BASE_URL must be https:// (got ${JSON.stringify(
        registryBaseUrl,
      )}). Override with SCOPEBALL_ALLOW_INSECURE_REGISTRY=1 only for local smoke tests.`,
    );
  }
}

const prodOverrides = {
  mode: "production",
  devtool: false,
  optimization: { minimize: true },
};

module.exports = common.map((cfg) => merge(cfg, prodOverrides));
