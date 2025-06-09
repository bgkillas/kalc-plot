const path = require("path");
const CopyPlugin = require("copy-webpack-plugin");

module.exports = {
  mode: "production",
  entry: "./bootstrap.js",
  output: {
    path: path.resolve(__dirname, "dist"),
    filename: "bundle.js",
  },
  experiments: {
    asyncWebAssembly: true,
  },
  plugins: [
    new CopyPlugin({
      patterns: [
        { from: "index.html", to: "." },
      ],
    }),
  ],
  devServer: {
    static: {
      directory: __dirname,
    },
    compress: true,
    port: 8080,
  },
};