#!/usr/bin/env node

"use strict";

const https = require("https");
const http = require("http");
const fs = require("fs");
const path = require("path");
const os = require("os");

const REPO = "Lynricsy/GrokSearchMCP";
const VERSION = require("../package.json").version;

const PLATFORM_MAP = {
  "linux-x64": "linux-x64",
  "linux-arm64": "linux-arm64",
  "darwin-x64": "darwin-x64",
  "darwin-arm64": "darwin-arm64",
  "win32-x64": "win32-x64",
};

function getPlatformKey() {
  const key = `${os.platform()}-${os.arch()}`;
  if (!PLATFORM_MAP[key]) {
    console.error(
      `Unsupported platform: ${key}\n` +
        `Supported: ${Object.keys(PLATFORM_MAP).join(", ")}\n` +
        `You can build from source: cargo build --release`
    );
    process.exit(1);
  }
  return PLATFORM_MAP[key];
}

function fetch(url, maxRedirects = 5) {
  return new Promise((resolve, reject) => {
    if (maxRedirects <= 0) {
      return reject(new Error("Too many redirects"));
    }

    const client = url.startsWith("https") ? https : http;
    client
      .get(url, { headers: { "User-Agent": "grok-search-mcp-installer" } }, (res) => {
        if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
          return fetch(res.headers.location, maxRedirects - 1)
            .then(resolve)
            .catch(reject);
        }
        if (res.statusCode !== 200) {
          return reject(new Error(`HTTP ${res.statusCode} for ${url}`));
        }
        const chunks = [];
        res.on("data", (chunk) => chunks.push(chunk));
        res.on("end", () => resolve(Buffer.concat(chunks)));
        res.on("error", reject);
      })
      .on("error", reject);
  });
}

async function main() {
  const platformKey = getPlatformKey();
  const ext = os.platform() === "win32" ? ".exe" : "";
  const filename = `grok-search-mcp-${platformKey}${ext}`;
  const url = `https://github.com/${REPO}/releases/download/v${VERSION}/${filename}`;

  const binDir = path.join(__dirname, "..", "bin");
  const binPath = path.join(binDir, `grok-search-mcp${ext}`);

  // 如果二进制文件已存在且可执行，跳过下载
  if (fs.existsSync(binPath)) {
    try {
      fs.accessSync(binPath, fs.constants.X_OK);
      console.log(`grok-search-mcp already installed at ${binPath}`);
      return;
    } catch {
      // 文件存在但不可执行，重新下载
    }
  }

  console.log(`Downloading ${filename} from GitHub Releases...`);

  if (!fs.existsSync(binDir)) {
    fs.mkdirSync(binDir, { recursive: true });
  }

  try {
    const data = await fetch(url);
    fs.writeFileSync(binPath, data);
    if (os.platform() !== "win32") {
      fs.chmodSync(binPath, 0o755);
    }
    console.log(`Installed grok-search-mcp to ${binPath}`);
  } catch (err) {
    console.error(`Failed to download binary: ${err.message}`);
    console.error(`URL: ${url}`);
    console.error(
      "You can build from source instead:\n" +
        "  git clone https://github.com/Lynricsy/GrokSearchMCP.git\n" +
        "  cd GrokSearchMCP && cargo build --release"
    );
    process.exit(1);
  }
}

main();
