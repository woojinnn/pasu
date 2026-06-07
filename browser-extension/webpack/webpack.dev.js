const { merge } = require("webpack-merge");
process.env.NODE_ENV = process.env.NODE_ENV || "development";
const common = require("./webpack.common.js");

const devOverrides = {
  mode: "development",
  devtool: "cheap-module-source-map",
  watch: true,
  watchOptions: { ignored: /node_modules/ },
};

module.exports = common.map((cfg) => merge(cfg, devOverrides));
