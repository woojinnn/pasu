const path = require("path");
const webpack = require("webpack");
const CopyPlugin = require("copy-webpack-plugin");
const Dotenv = require("dotenv-webpack");
const WextManifestWebpackPlugin = require("wext-manifest-webpack-plugin");

const targetBrowser = process.env.TARGET_BROWSER || "chrome";
const sourceDir = path.resolve(__dirname, "..", "src");
const distDir = path.resolve(__dirname, "..", "dist", targetBrowser);

// Shared bits of the webpack config — the actual exported configs differ
// only in `entry`, `target`, and which build-time plugins they own.
const sharedResolve = {
  extensions: [".ts", ".tsx", ".js", ".json"],
  alias: {
    "@lib": path.resolve(sourceDir, "lib"),
    "@background": path.resolve(sourceDir, "background"),
  },
  fallback: {
    buffer: require.resolve("buffer/"),
    process: require.resolve("process/browser"),
  },
};

const sharedModule = {
  rules: [
    {
      type: "javascript/auto",
      test: /manifest\.json$/,
      use: {
        loader: "wext-manifest-loader",
        options: { usePackageJSONVersion: true },
      },
      exclude: /node_modules/,
    },
    {
      test: /\.tsx?$/,
      loader: "ts-loader",
      exclude: /node_modules/,
    },
    {
      test: /\.css$/,
      use: ["style-loader", "css-loader"],
    },
    {
      test: /\.wasm$/,
      type: "asset/resource",
    },
  ],
};

const sharedPlugins = () => [
  new Dotenv({
    path: path.resolve(__dirname, "..", ".env"),
    safe: false,
    silent: true,
  }),
  // ProvidePlugin for `process` so readable-stream's `process.nextTick` etc.
  // resolve at runtime even in code paths that don't import it explicitly.
  new webpack.ProvidePlugin({ process: "process/browser" }),
];

// Page/contentscript build — content scripts run in page context, popup +
// confirm run in extension-page context. Default `target` ("web") is the
// right choice; webpack's chunk loader can use `document.*` here.
//
// This config owns `clean: true` so it must run FIRST. The SW build
// declares `dependencies: ["pages"]` to enforce the order so it doesn't
// race against the dist wipe.
const pageConfig = {
  name: "pages",
  target: "web",
  entry: {
    "content-scripts/inject-scripts": path.join(
      sourceDir,
      "content-scripts",
      "inject-scripts.ts",
    ),
    "content-scripts/window-ethereum-messages": path.join(
      sourceDir,
      "content-scripts",
      "window-ethereum-messages.ts",
    ),
    "content-scripts/bypass-check": path.join(
      sourceDir,
      "content-scripts",
      "bypass-check.ts",
    ),
    "injected/proxy-injected-providers": path.join(
      sourceDir,
      "injected",
      "proxy-injected-providers.ts",
    ),
    "confirm/index": path.join(sourceDir, "confirm", "index.ts"),
    "popup/index": path.join(sourceDir, "popup", "index.ts"),
    manifest: path.join(sourceDir, "manifest.json"),
  },
  output: {
    filename: "js/[name].js",
    path: distDir,
    clean: true,
  },
  resolve: sharedResolve,
  experiments: {
    asyncWebAssembly: true,
  },
  module: sharedModule,
  plugins: [
    ...sharedPlugins(),
    new WextManifestWebpackPlugin(),
    new CopyPlugin({
      patterns: [
        { from: path.resolve(__dirname, "..", "public"), to: distDir },
      ],
    }),
  ],
};

// SW build — `target: "webworker"` is required so webpack does NOT emit
// `document.baseURI` / `document.createElement` in the runtime chunk
// loader. Those references would crash the SW at registration time
// (Service worker registration failed, status code 15).
//
// Runs after `pages` so the dist wipe doesn't clobber `js/background.js`.
const swConfig = {
  name: "sw",
  target: "webworker",
  dependencies: ["pages"],
  entry: {
    background: path.join(sourceDir, "background", "index.ts"),
  },
  output: {
    filename: "js/[name].js",
    path: distDir,
  },
  resolve: sharedResolve,
  experiments: {
    asyncWebAssembly: true,
  },
  module: sharedModule,
  plugins: sharedPlugins(),
};

module.exports = [pageConfig, swConfig];
