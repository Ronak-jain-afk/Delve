#!/usr/bin/env node
const { createWriteStream, existsSync, mkdirSync } = require('fs');
const { chmod } = require('fs/promises');
const { get } = require('https');
const { platform, arch } = process;
const path = require('path');
const { pipeline } = require('stream/promises');

const BINARY_NAME = platform === 'win32' ? 'delve-core.exe' : 'delve-core';

const TRIPLES = {
  'darwin-x64': 'x86_64-apple-darwin',
  'darwin-arm64': 'aarch64-apple-darwin',
  'linux-x64': 'x86_64-unknown-linux-gnu',
  'linux-arm64': 'aarch64-unknown-linux-gnu',
  'win32-x64': 'x86_64-pc-windows-msvc',
  'win32-arm64': 'aarch64-pc-windows-msvc',
};

const triple = TRIPLES[`${platform}-${arch}`];
if (!triple) {
  console.error(`Unsupported platform: ${platform}-${arch}`);
  process.exit(1);
}

const pkg = require('../package.json');
const version = pkg.version;

const binDir = path.join(__dirname, '..', 'node_modules', '.bin');
const binaryPath = path.join(binDir, BINARY_NAME);

if (existsSync(binaryPath)) {
  process.exit(0);
}

async function download() {
  const ext = platform === 'win32' ? '.exe' : '';
  const url = `https://github.com/Ronak-jain-afk/Delve/releases/download/v${version}/delve-core-${triple}${ext}`;

  console.log(`Downloading delve-core v${version} for ${platform}-${arch}...`);

  const res = await new Promise((resolve, reject) => {
    const req = get(url, (res) => {
      if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
        get(res.headers.location, resolve).on('error', reject);
        return;
      }
      if (res.statusCode === 404) {
        reject(new Error(`Not found at ${url}`));
        return;
      }
      if (res.statusCode !== 200) {
        reject(new Error(`HTTP ${res.statusCode}`));
        return;
      }
      resolve(res);
    });
    req.on('error', reject);
    req.end();
  });

  if (!existsSync(binDir)) {
    mkdirSync(binDir, { recursive: true });
  }

  await pipeline(res, createWriteStream(binaryPath));
  await chmod(binaryPath, 0o755);
  console.log(`Installed delve-core at ${binaryPath}`);
}

download().catch((err) => {
  console.error(`Failed to download delve-core binary: ${err.message}`);
  console.error('');
  console.error('To build from source:');
  console.error('  git clone https://github.com/Ronak-jain-afk/Delve.git');
  console.error('  cd Delve/crates/delve-core && cargo build --release');
  process.exit(1);
});
