const { merge } = require("webpack-merge");
const common = require("./webpack.common.js");

const devOverrides = {
  mode: "development",
  devtool: "cheap-module-source-map",
  watch: true,
  watchOptions: { ignored: /node_modules/ },
};

module.exports = common.map((cfg) => merge(cfg, devOverrides));
