const { merge } = require("webpack-merge");
const common = require("./webpack.common.js");

const prodOverrides = {
  mode: "production",
  devtool: false,
  optimization: { minimize: true },
};

module.exports = common.map((cfg) => merge(cfg, prodOverrides));
