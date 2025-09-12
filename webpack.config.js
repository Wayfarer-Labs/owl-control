const path = require("path");
const HtmlWebpackPlugin = require("html-webpack-plugin");

// Common config for both main and renderer processes
const commonConfig = {
  mode: process.env.NODE_ENV === "development" ? "development" : "production",
  devtool: process.env.NODE_ENV === "development" ? "source-map" : false,
  resolve: {
    extensions: [".ts", ".tsx", ".js", ".jsx"],
    alias: {
      "@": path.resolve(__dirname, "src"),
    },
  },
  module: {
    rules: [
      {
        test: /\.(tsx?|jsx?)$/,
        use: "ts-loader",
        exclude: /node_modules/,
      },
      {
        test: /\.css$/,
        use: ["style-loader", "css-loader", "postcss-loader"],
      },
      {
        test: /\.(png|jpg|gif|svg)$/,
        type: "asset/resource",
        generator: {
          filename: "assets/[name][ext]",
        },
      },
    ],
  },
};

// Main process config (Electron)
const mainConfig = {
  ...commonConfig,
  target: "electron-main",
  entry: "./src/main.ts",
  output: {
    path: path.resolve(__dirname, "dist"),
    filename: "main.js",
  },
  node: {
    __dirname: false,
    __filename: false,
  },
};

// Renderer process config (React)
const rendererConfig = {
  ...commonConfig,
  target: "electron-renderer",
  entry: "./src/index.js",
  output: {
    path: path.resolve(__dirname, "dist"),
    filename: "index.js",
  },
  plugins: [
    new HtmlWebpackPlugin({
      template: "./src/index.html",
      filename: "index.html",
    }),
  ],
};

// Preload script config
const preloadConfig = {
  ...commonConfig,
  target: "electron-preload",
  entry: "./src/preload.js",
  output: {
    path: path.resolve(__dirname, "dist"),
    filename: "preload.js",
  },
};

// Export configs as an array
module.exports = [mainConfig, rendererConfig, preloadConfig];
