const path = require("path");
const CopyPlugin = require("copy-webpack-plugin");
const WasmPackPlugin = require("@wasm-tool/wasm-pack-plugin");

const dist = path.resolve(__dirname, "dist");

module.exports = {
  mode: "development",
  entry: {
    index: "./js/index.js"
  },
  output: {
    path: dist,
    filename: "[name].js"
  },
  module: {
    rules: [
      {
        test: /\.js$/,
        loader: 'babel-loader',
        exclude: /node_modules/
      },
    ]
  },
  devServer: {
    static: {
      directory: dist
    }
  },
  plugins: [
    new CopyPlugin({
      patterns: [
        { from: path.resolve(__dirname, "static") }
      ]
    }),

    new WasmPackPlugin({
      crateDirectory: __dirname,
      forceMode: "production"
    }),
  ],
  resolve: {
    fallback: { "path": require.resolve("path-browserify") }
  },
  experiments: {
    asyncWebAssembly: true
  },
  performance: {
    hints: false
  }
};
