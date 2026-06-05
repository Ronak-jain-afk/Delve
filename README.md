# Delve

**Static analysis for JS/TS that doesn't suck.**

Delve is a CLI tool that finds dead code, giant functions, duplicates, and risky patterns in your JavaScript and TypeScript projects. Zero config, no LSP, no daemon — just fast static analysis.

## Install

```bash
npm install -g @delve/cli
delve audit
```

Or build from source:

```bash
cargo build --release -p delve-core
./target/release/delve-core audit
```

## Commands

| Command | What it does |
|---------|-------------|
| `delve audit` | Full report: unused code, giant functions, duplicates, risky patterns, health score |
| `delve deadcode` | Unused exports only |
| `delve split` | Giant functions with line ranges + complexity |
| `delve dup` | Duplicate code blocks with locations |
| `delve health` | Single 0–100 score + todo list |

All accept `--json` for CI/tooling.

## Usage

```bash
# Run from project root (auto-detects entry points)
delve audit

# JSON output for CI
delve audit --json

# Specific analysis
delve deadcode
delve split
delve dup
delve health

# Analyze a subdirectory
delve audit --path src/

# Custom config
delve audit --config .delve.json
```

## Config

Create a `.delve.json` in your project root:

```json
{
  "thresholds": {
    "warningLines": 40,
    "criticalLines": 80,
    "warningComplexity": 10,
    "criticalComplexity": 20
  },
  "weights": {
    "unusedFile": 15,
    "giantCritical": 5,
    "giantWarning": 2,
    "duplicate": 3,
    "anyType": 1,
    "consoleLog": 1
  },
  "ignore": ["packages/", "test-fixtures/"],
  "entryPoints": ["src/index.ts"]
}
```

All fields are optional — Delve uses sensible defaults.

## Opt-out of false positives

```js
/* delve:used */
export function myFunction() { ... }
```

This silences unused-code warnings for the export below the comment.

## How it works

Delve uses [tree-sitter](https://tree-sitter.github.io/tree-sitter/) to parse source files into ASTs, then:

1. **Dead code** — builds an import dependency graph, marks exports reachable from entry points (`package.json` fields, well-known filenames, `require.main === module`), reports the rest
2. **Giant functions** — counts logical lines (skipping comments/blank lines/braces) and computes cyclomatic complexity (`if`, `for`, `while`, `case`, `&&`, `||`, `?:`, `catch`)
3. **Duplicates** — normalizes identifiers → `$id`, strings → `$str`, numbers → `$num`, then hashes 6–15 token sliding windows across files
4. **Risk patterns** — regex-free detection of `any` types, `console.log`, `debugger`, deep nesting (>4), long params (>5)
5. **Health score** — weighted 0–100 based on all findings; ≥70 green, 40–69 yellow, <40 red

## License

MIT
