#!/usr/bin/env node
const { execFileSync } = require('child_process');
const path = require('path');
const fs = require('fs');

const BINARY_NAME = process.platform === 'win32' ? 'delve-core.exe' : 'delve-core';

function findBinary() {
  const searchPaths = [
    path.join(__dirname, '..', 'node_modules', '.bin', BINARY_NAME),
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
  console.error('delve-core binary not found. Run: npm install -g @delve/cli');
  process.exit(1);
}

try {
  execFileSync(binary, process.argv.slice(2), { stdio: 'inherit' });
} catch (e) {
  process.exit(e.status ?? 1);
}
