const path = require("path");

const DEFAULT_SERVER_URL = "http://127.0.0.1:8788";

function buildMode(env = process.env) {
  return env.PASU_EXTENSION_BUILD_MODE || env.NODE_ENV || "development";
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
  return env.PASU_SERVER_URL || DEFAULT_SERVER_URL;
}

module.exports = {
  DEFAULT_SERVER_URL,
  buildMode,
  envFileNameForMode,
  envPathForMode,
  loadBuildEnv,
  resolveServerUrl,
};
