#!/usr/bin/env node

"use strict";

const { spawn } = require("child_process");
const path = require("path");
const os = require("os");

const ext = os.platform() === "win32" ? ".exe" : "";
const binPath = path.join(__dirname, "..", "bin", `grok-search-mcp${ext}`);

const child = spawn(binPath, process.argv.slice(2), {
  stdio: "inherit",
});

child.on("error", (err) => {
  if (err.code === "ENOENT") {
    console.error(
      "grok-search-mcp binary not found.\n" +
        'Run "npm rebuild groks" or reinstall the package.'
    );
  } else {
    console.error(`Failed to start grok-search-mcp: ${err.message}`);
  }
  process.exit(1);
});

child.on("exit", (code) => {
  process.exit(code ?? 1);
});
