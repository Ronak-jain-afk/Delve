#!/usr/bin/env node
const { platform, arch } = process;
console.log(`@delve/cli postinstall: platform=${platform} arch=${arch} — binary download not yet implemented`);
console.log('Build from source: cd crates/delve-core && cargo build --release');
