const path = require("path");

const DEFAULT_SERVER_URL = "https://dambi-policy.duckdns.org";

function buildMode(env = process.env) {
  return env.DAMBI_EXTENSION_BUILD_MODE || env.NODE_ENV || "development";
}

function envFileNameForMode(mode = buildMode()) {
  return mode === "production" ? ".env" : `.env.${mode}`;
}

function envPathForMode(extRoot, mode = buildMode()) {
  return path.join(extRoot, envFileNameForMode(mode));
}

function loadBuildEnv(extRoot, mode = buildMode()) {
  require("dotenv").config({ path: envPathForMode(extRoot, mode) });
}

function resolveServerUrl(env = process.env) {
  return env.DAMBI_SERVER_URL || DEFAULT_SERVER_URL;
}

// Channel-specific PINNED registry-bundle signing public key (SPKI base64). The
// SW verifies each bundle's detached ECDSA P-256 signature against this before
// installing the decoder. Empty when signing is not yet pinned on this channel.
function resolvePinnedBundleKey(env = process.env) {
  return env.PINNED_BUNDLE_PUBLIC_KEY || "";
}

// Whether bundle signatures are ENFORCED on this build channel. Baked verbatim
// as the string "true"/"false"; the verifier reads `=== "true"`. Off by default
// so an unsigned dev/staging registry keeps working (staged rollout).
function resolveRequireBundleSig(env = process.env) {
  return env.DAMBI_REQUIRE_BUNDLE_SIGNATURE === "true" ? "true" : "false";
}

module.exports = {
  DEFAULT_SERVER_URL,
  buildMode,
  envFileNameForMode,
  envPathForMode,
  loadBuildEnv,
  resolveServerUrl,
  resolvePinnedBundleKey,
  resolveRequireBundleSig,
};
