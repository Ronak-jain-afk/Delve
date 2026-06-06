# Delve — Agent Guide

## Project structure

```
crates/delve-core/   Rust binary — parsing, analysis, JSON report output
packages/cli/        npm wrapper — downloads prebuilt binary, CLI entrypoint
test-fixtures/       Sample JS/TS codebases for testing
```

## Architecture

- Rust binary (`delve-core`) does all parsing/analysis.
- npm package (`@glimpsecode/cli`) spawns the binary.
- Tree-sitter parses TS/JS/JSX/TSX/MJS/CJS. Rayon for parallelism.
- No LSP, no watch mode, no auto-refactoring — pure static analysis CLIs.

## Commands (MVP)

| Command | What it does |
|---------|-------------|
| `glimpse audit` | Full report: unused code, giant functions, duplicates, risky patterns, health score |
| `glimpse deadcode` | Unused exports only |
| `glimpse split` | Giant functions with line ranges + complexity |
| `glimpse dup` | Duplicate code blocks with locations |
| `glimpse health` | Single 0–100 score + todo list |

All accept `--json` for CI/tooling.

## Key conventions

- **First, do `cargo build --release`** — binary goes in `crates/delve-core/target/release/delve-core`
- **Tests**: Rust unit tests in each module (`cargo test`). No integration test runner yet.
- **Config**: `.delve.json` at project root overrides thresholds and weights.
- **Comment-based opt-out**: `/* delve:used */` silences unused-code false positives.
- **Entry points are auto-detected** from `package.json` (`main`/`module`/`bin`/`exports`), well-known filenames (`index.ts`, `main.ts`), and `require.main === module` patterns.
- **`node_modules` is always skipped** — only project source is analyzed.

## When scaffolding this repo

1. Create Rust crate: `cargo init --lib --name delve-core crates/delve-core`
2. Create npm package: `npm init` in `packages/cli/`
3. Add workspace `Cargo.toml`
4. Set up GitHub Actions cross-compile (6 targets: linux/macos/windows × x64/arm64)

## v2 Roadmap (priority order)

| Priority | Feature | Phase |
|----------|---------|-------|
| P0 | Barrel file detection, type-only imports, circular deps | 14 |
| P0 | SARIF output, GH annotations, exit code semantics | 17 |
| P1 | ~~Jaccard near-duplicate detection~~ | 13 |
| P1 | Unused package.json dependency detection | 14 |
| P1 | Basic `--fix` for unused code (add delve:used comments) | 16 |
| P1 | Incremental parsing + AST caching | 18 |
| P2 | Catastrophic anti-patterns (any propagation, async misuse, state smells, secrets) | 15 |
| P2 | HTML report | 17 |
| P2 | Benchmark suite | 18 |
| P2 | Trend tracking + history | 20 |
| P3 | Plugin system (Rust + JS rules) | 19 |
| P3 | Interactive TUI mode | 16 |
| P3 | Diff analysis, summary badge, machine output formats | 20 |
| P3 | Large project optimizations (lazy loading, streaming) | 18 |

Refer to `TASK_PLAN.md` for detailed task breakdown per phase.

## Gotchas

- Tree-sitter needs separate grammars for TS and JS — both must be loaded.
- Import resolution needs extension-trying order: `.ts` → `.tsx` → `.js` → `.jsx` → `.mjs` → `.cjs` → `index.*`
- Cyclomatic complexity must count `&&`, `||`, `?.`, `??`, ternaries, `case` in addition to `if`/`for`/`while`.
- Duplicate detection: normalize identifiers → `$id`, strings → `$str`, numbers → `$num` before hashing.
- Jaccard near-duplicate: tokenize, then use set intersection/union on n-grams, threshold ≥ 0.7.
- `console.log` detection should skip `__tests__/` and `*.test.*`/`*.spec.*` files.
- Health score floors at 0; >= 70 green ("healthy"), 40–69 yellow ("needs work"), < 40 red ("vibe disaster").
- Circular dependency: use Kosaraju or Tarjan SCC algorithm on the directed import graph.
