#!/usr/bin/env node
const { execFileSync } = require('child_process');
const path = require('path');
const fs = require('fs');

const BINARY_NAME = process.platform === 'win32' ? 'delve-core.exe' : 'delve-core';

function findBinary() {
  const searchPaths = [
    path.join(__dirname, '..', 'node_modules', '.bin', BINARY_NAME),
    path.join(__dirname, '..', '..', 'cli-darwin-x64', BINARY_NAME),
    path.join(__dirname, '..', '..', 'cli-darwin-arm64', BINARY_NAME),
    path.join(__dirname, '..', '..', 'cli-linux-x64', BINARY_NAME),
    path.join(__dirname, '..', '..', 'cli-linux-arm64', BINARY_NAME),
    path.join(__dirname, '..', '..', 'cli-win32-x64', BINARY_NAME),
    path.join(__dirname, '..', '..', 'cli-win32-arm64', BINARY_NAME),
    path.join(__dirname, '..', '..', '..', 'target', 'release', BINARY_NAME),
    path.join(__dirname, '..', '..', '..', 'target', 'debug', BINARY_NAME),
  ];
  for (const p of searchPaths) {
    if (fs.existsSync(p)) return p;
  }
  return null;
}

const binary = findBinary();
if (!binary) {
  console.error('delve-core binary not found.');
  console.error('Install: npm install -g @ronak-jain-afk/cli');
  console.error('Build: cd crates/delve-core && cargo build --release');
  process.exit(1);
}

try {
  execFileSync(binary, process.argv.slice(2), { stdio: 'inherit' });
} catch (e) {
  process.exit(e.status ?? 1);
}
