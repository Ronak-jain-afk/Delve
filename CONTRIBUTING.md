# Contributing

## Prerequisites

- Rust 1.70+ (edition 2021)
- Node.js 18+ (for npm package)

## Building

```bash
# Build the Rust binary
cargo build --release -p delve-core
./crates/delve-core/target/release/delve-core audit

# Or build all workspace members
cargo build --release --workspace
```

## Testing

```bash
# Run all Rust unit tests
cargo test -p delve-core

# Run tests with output
cargo test -p delve-core -- --nocapture
```

Tests are in each module under `#[cfg(test)] mod tests`. Test fixtures live in `test-fixtures/vibe-app/`.

## Project structure

```
crates/delve-core/       Rust binary — parsing, analysis, JSON report
packages/cli/            npm wrapper — downloads prebuilt binary, CLI entry
packages/cli-*/          Platform-specific npm packages (darwin-x64, linux-arm64, etc.)
test-fixtures/           Sample JS/TS codebases for testing
```

## Architecture

- Tree-sitter parses TS/JS/TSX/MJS/CJS files
- Rayon for parallel file processing
- Import resolution tries extensions in order: `.ts` → `.tsx` → `.js` → `.jsx` → `.mjs` → `.cjs` → `index.*`
- Entry points auto-detected from `package.json` (`main`/`module`/`bin`/`exports`), well-known filenames (`index.ts`, `main.ts`), and `require.main === module` patterns
- `node_modules` is always skipped via `.gitignore` filtering
- Config loaded from `.delve.json` (project root) or `--config` path; CLI flags override file values

## Conventions

- Use `edition = "2021"` in Cargo.toml (not 2024) for tree-sitter compatibility
- Canonicalize all file paths with `canonicalize()` for consistent HashMap lookups
- Add `_with_ignore` function variants when a function needs config-aware filtering
- Health score floors at 0; >= 70 green, 40–69 yellow, < 40 red

## Release process

1. Tag a version: `git tag v0.1.0 && git push --tags`
2. The release workflow cross-compiles for 6 targets (linux/macos/windows × x64/arm64)
3. SHA-256 checksums are generated for each binary
4. Binaries are uploaded to the GitHub Release
5. All 7 npm packages are published (`@ronak-jain-afk/cli`, `@ronak-jain-afk/cli-darwin-x64`, etc.)
