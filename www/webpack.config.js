const path = require("path");

module.exports = {
  mode: "production",
  entry: "./bootstrap.js",
  output: {
    path: path.resolve(__dirname),
    filename: "bundle.js",
  },
  experiments: {
    asyncWebAssembly: true,
  },
  plugins: [
  ],
  devServer: {
    static: {
      directory: __dirname,
    },
    compress: true,
    port: 8080,
 },
};