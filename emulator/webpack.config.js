/* eslint-env node */

const express = require("express");
const CopyPlugin = require("copy-webpack-plugin");
const WasmPackPlugin = require("@wasm-tool/wasm-pack-plugin");
const path = require("node:path");
const fs = require("node:fs/promises");
const { Buffer } = require("node:buffer");

const dist = path.resolve(__dirname, "dist");

module.exports = {
  mode: "development",
  entry: {
    index: "./js/index.js",
  },
  output: {
    path: dist,
    filename: "[name].js",
  },
  module: {
    rules: [
      {
        test: /\.js$/,
        loader: "babel-loader",
        exclude: /node_modules/,
      },
    ],
  },
  devServer: {
    static: {
      directory: dist,
    },
    onBeforeSetupMiddleware: (devServer) => {
      if (!devServer) {
        throw new Error("webpack-dev-server is not defined");
      }

      devServer.app.get("/disk", async (req, res) => {
        let ranges = req.range(10 * 1024 * 1024);
        if (!ranges || ranges.length > 1) {
          res.status(400).json({ message: "Expecting 1 range" });
          return;
        }

        const range = ranges[0];

        if (!range || range < 0) {
          res.status(416).json({ message: "Invalid range" });
          return;
        }

        const { start, end } = range;

        const f = await fs.open("img.fat", "r");
        const buf = Buffer.alloc(end - start);

        await f.read(buf, 0, end - start, start);
        await f.close();

        res.status(206).send(buf);
      });

      devServer.app.patch(
        "/disk",
        express.raw({
          inflate: true,
          limit: "10mb",
          type: "application/octet-stream",
        }),
        async (req, res) => {
          let ranges = req.range(10 * 1024 * 1024);
          if (!ranges || ranges.length > 1) {
            res.status(400).json({ message: "Expecting 1 range" });
            return;
          }

          const range = ranges[0];

          if (!range || range < 0) {
            res.status(416).json({ message: "Invalid range" });
            return;
          }

          const { start, end } = range;

          const f = await fs.open("img.fat", "r+");

          console.log('w', req.body, start);

          try {
            await f.write(req.body, 0, null, start);
          } finally {
            await f?.see
            await f?.close();
          }
          res.status(200).json({'written': end - start});
        }
      );
    },
  },
  plugins: [
    new CopyPlugin({
      patterns: [{ from: path.resolve(__dirname, "static") }],
    }),

    new WasmPackPlugin({
      crateDirectory: __dirname,
      forceMode: "production",
    }),
  ],
  resolve: {
    fallback: { path: require.resolve("path-browserify") },
  },
  experiments: {
    asyncWebAssembly: true,
  },
  performance: {
    hints: false,
  },
};
