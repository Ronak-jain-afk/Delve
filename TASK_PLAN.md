# Delve - Detailed Development Task Plan

## Phase 0: Project Scaffolding & Tooling

### 0.1 Initialize Rust crate (`delve-core`)
- Create Cargo project with `cargo init --lib --name delve-core` in `crates/delve-core/`
- Configure `Cargo.toml` with dependencies:
  - `tree-sitter` (with `typescript` and `javascript` language grammars)
  - `tree-sitter-traversal` for AST walking utilities
  - `rayon` for parallel processing
  - `serde` + `serde_json` for JSON output
  - `clap` for CLI argument parsing
  - `yansi` or `colored` for terminal output
  - `walkdir` for recursive file discovery
  - `ignore` for `.gitignore`-aware file walking
- Set up `rustfmt` config and `clippy` as a lint step

### 0.2 Initialize npm package (`@glimpsecode/cli`)
- `npm init` in `packages/cli/`
- Set up `bin` entry point (`bin/delve.js`)
- Add `postinstall` script placeholder
- Configure `package.json` with `name: "@glimpsecode/cli"`, `version: "0.1.0"`, `bin: { delve: "bin/delve.js" }`
- Add `meow` or `yargs` for CLI argument parsing in the JS wrapper
- Add `chalk` or `picocolors` for colored terminal output in the JS wrapper

### 0.3 Set up workspace structure
```
delve/
├── crates/
│   └── delve-core/        # Rust binary crate
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs
│           ├── lib.rs
│           ├── parser.rs
│           ├── graph.rs
│           ├── unused.rs
│           ├── giant_funcs.rs
│           ├── duplicates.rs
│           ├── risks.rs
│           ├── health.rs
│           └── report.rs
├── packages/
│   └── cli/               # npm wrapper
│       ├── package.json
│       └── bin/
│           └── delve.js
├── test-fixtures/          # Sample JS/TS codebases for testing
│   └── vibe-app/
├── Cargo.toml              # Workspace Cargo.toml
├── .github/
│   └── workflows/
│       └── release.yml     # Cross-compile CI
├── README.md
└── TASK_PLAN.md
```

### 0.4 Create test fixtures
- Small TypeScript project with known unused exports, giant functions, duplicates, and risky patterns
- Small JavaScript project with similar issues
- Edge cases: single file, empty project, project with only type definitions
- Add a `package.json` with `main`, `module`, and `exports` fields for entry point testing

### 0.5 Set up CI
- GitHub Actions workflow for Rust build + test on Linux, macOS, Windows
- GitHub Actions workflow for npm package tests (lint, dry-run publish)
- Cross-compilation matrix for `x86_64`, `aarch64` Linux, macOS, Windows

---

## Phase 1: Core Parsing & AST Foundation

### 1.1 Implement file discovery (`delve-core/src/lib.rs` or new module)
- Recursively walk directories
- Filter by extensions: `.ts`, `.tsx`, `.js`, `.jsx`, `.mjs`, `.cjs`
- Respect `.gitignore` (use `ignore` crate)
- Accept both file paths and directory paths as CLI arguments
- Return list of absolute file paths

### 1.2 Set up tree-sitter parsers (`delve-core/src/parser.rs`)
- Initialize tree-sitter with TypeScript and JavaScript language grammars
- Create a function `parse_file(path: &str) -> Option<tree_sitter::Tree>`
- Handle parse errors gracefully (return partial results, don't crash)
- Cache parsed trees by file path for reuse across analysis passes

### 1.3 Extract top-level exports (`delve-core/src/parser.rs`)
- Walk AST to find:
  - `export function`, `export const`, `export class`, `export interface`
  - `export default function/class`
  - `export { ... }` (named exports)
  - `export * from` (re-exports)
  - `module.exports = `, `exports.` (CommonJS)
- Record: symbol name, kind (function/const/class/type), start line, end line, file path
- Handle `export { foo as bar }` aliasing

### 1.4 Extract imports (`delve-core/src/parser.rs`)
- Walk AST to find:
  - `import { x } from 'y'`, `import x from 'y'`, `import * as x from 'y'`
  - `import('y')` (dynamic — log but don't resolve statically)
  - `require('y')` (CommonJS)
- Record: imported symbol names, source module path, file path, line number
- Distinguish default vs named imports

### 1.5 Extract function definitions (`delve-core/src/parser.rs`)
- Walk AST for:
  - Function declarations (`function foo() {}`)
  - Function expressions assigned to variables/consts
  - Arrow functions assigned to variables/consts
  - Method definitions in classes
  - Exported functions (already covered by export extraction, unify)
- Record: function name (or anonymous placeholder), start line, end line, file path, parent scope

### 1.6 Build per-file symbol table (`delve-core/src/parser.rs`)
- For each file, create a struct containing:
  - `file_path: String`
  - `exports: Vec<Export>` (name, kind, line, is_used bool)
  - `imports: Vec<Import>` (symbols, source)
  - `functions: Vec<FunctionInfo>` (name, start, end, body_start, lines_of_code, complexity)
- Store in a `HashMap<String, FileSymbols>` keyed by file path

### 1.7 Unit tests for phase 1
- Test file discovery with glob patterns and exclude patterns
- Test parsing of basic TS/JS files
- Test export extraction (every export form listed above)
- Test import extraction (every import form listed above)
- Test function extraction
- Test edge cases: empty files, syntax errors, files with only comments

---

## Phase 2: Import Resolution & Dependency Graph

### 2.1 Resolve relative imports (`delve-core/src/graph.rs`)
- Given `import { x } from './foo'`, resolve to an absolute file path:
  - Try `./foo.ts`, `./foo.tsx`, `./foo.js`, `./foo.jsx`, `./foo.mjs`, `./foo.cjs`
  - Try `./foo/index.ts`, `./foo/index.tsx`, `./foo/index.js` etc.
  - Try `./foo/package.json` → `main`/`module` field
- Handle `..` and `.` in paths
- Return `None` for unresolvable imports (log warning, continue)

### 2.2 Resolve package imports (`delve-core/src/graph.rs`)
- Given `import { x } from 'lodash'`, resolve by:
  - Look up `node_modules/<package>/package.json` → `main`/`module`/`exports` field
  - Follow the resolved path to find the entry file
- Handle scoped packages (`@scope/package`)
- Handle deep imports (`lodash/merge` → `node_modules/lodash/merge.js`)
- If `node_modules` not found, log warning and skip

### 2.3 Build global export-import graph (`delve-core/src/graph.rs`)
- For each file, create edges from import symbol → export symbol in the resolved source file
- Track unresolved imports separately (mark as "external library")
- Handle re-exports: `export { foo } from './bar'` creates an alias edge
- Handle `export * from './bar'` — create edges for all bar's exports
- Store as `HashMap<FileKey, HashMap<SymbolName, Vec<IncomingRef>>>`

### 2.4 Identify entry points (`delve-core/src/graph.rs`)
- Heuristic 1: Read `package.json` in project root — extract `main`, `module`, `bin`, `exports` fields → resolve to absolute paths
- Heuristic 2: Check for files named `index.ts`, `main.ts`, `cli.ts`, `app.ts` in project root
- Heuristic 3: Scan all files for patterns `if (require.main === module)` or `if (import.meta.url === ...)`
- Return a `Vec<FilePath>` of entry point candidates

### 2.5 Traverse graph from entry points (`delve-core/src/graph.rs`)
- Starting from entry points, perform BFS/DFS to mark reachable exports
- For each visited file, mark all its exports as "reachable"
- For each import in visited files, follow edges to mark target exports as "reachable"
- Handle circular dependencies gracefully (use a visited set)
- Collect: definitely used exports, potentially unused exports

### 2.6 Unit tests for phase 2
- Test relative import resolution (all extension variants, index files)
- Test package import resolution (with mock node_modules)
- Test graph building with simple and complex dependency chains
- Test entry point detection heuristics
- Test graph traversal and reachability marking

---

## Phase 3: Unused Code Detection

### 3.1 Implement unused detection logic (`delve-core/src/unused.rs`)
- After graph traversal, collect all exports where `is_used == false`
- For each unused export, determine:
  - Symbol name
  - File path and line number
  - Kind (function, const, class, type, default export)
  - Whether the file is an entry point itself
- Handle `/* delve:used */` comment: scan for this comment on the line above any export and mark it as used
- Categorize results: "definitely unused" vs "maybe unused" (e.g., in files that might be loaded dynamically)

### 3.2 Generate unused code report (`delve-core/src/unused.rs`)
- Format output as list of `{file, line, symbol, kind}`
- Sort by file path for readability
- For `--json` flag, output as JSON array

### 3.3 Implement `deadcode` command (subset of audit)
- Reuse the unused detection logic
- Output format identical to audit's unused section
- Add optional `--remove` flag (future: dry-run mode that shows the `rm` commands)
- For MVP: just output the list, no removal

### 3.4 Tests for unused detection
- Test with test fixtures that have known unused exports
- Test that entry point exports are not reported
- Test that `/* delve:used */` silences false positives
- Test with TypeScript type-only exports
- Test with CommonJS `module.exports`
- Test with re-exports (`export * from`)

---

## Phase 4: Giant Functions & Complexity Analysis

### 4.1 Count lines of code per function (`delve-core/src/giant_funcs.rs`)
- For each function AST node, count lines between start and end
- Exclude blank lines (lines with only whitespace)
- Exclude comment lines (`//` and `/* */`)
- Exclude lines that are only `{` or `}`
- Store as `logical_lines` count

### 4.2 Compute cyclomatic complexity (`delve-core/src/giant_funcs.rs`)
- Walk the function's AST subtree counting complexity-incrementing nodes:
  - `if` statements (including `else if`)
  - `for`, `for-in`, `for-of` loops
  - `while`, `do-while` loops
  - `switch` cases (each `case` adds 1)
  - `catch` clauses
  - `&&` and `||` logical operators
  - `?.` optional chaining
  - `??` nullish coalescing
  - `? :` ternary conditional
  - `??=` nullish coalescing assignment
- Base complexity = 1, add 1 for each control flow node found

### 4.3 Apply threshold rules (`delve-core/src/giant_funcs.rs`)
- Read thresholds from config or use defaults:
  - Warning: > 40 lines OR complexity > 10
  - Critical: > 80 lines OR complexity > 20
- For each function, determine its severity level
- Provide `--threshold-warning` and `--threshold-critical` CLI flags

### 4.4 Implement `split` command
- Show only giant functions with their line ranges and metric values
- For each warning/critical function, suggest a refactoring note
- Include the actual code snippet lines for context (MVP: just show the line numbers)

### 4.5 Tests for giant functions & complexity
- Test line counting with comments, blank lines, braces
- Test complexity calculation for each control flow type
- Test threshold classification
- Test edge cases: one-line functions, deeply nested functions, IIFEs

---

## Phase 5: Duplicate Code Detection

### 5.1 Tokenize source code (`delve-core/src/duplicates.rs`)
- Use tree-sitter to get tokens (not just whitespace-split)
- Extract token types (identifier, keyword, operator, literal, punctuation)
- Normalize tokens:
  - All identifiers → `$id`
  - All string literals → `$str`
  - All numeric literals → `$num`
  - Comments removed entirely
  - Whitespace collapsed
- Keep original text ranges for reporting

### 5.2 Implement fingerprinting (`delve-core/src/duplicates.rs`)
- Slide a window of configurable size (default: 10 tokens minimum)
- For each window, compute a hash (use `std::hash::Hasher` or SHA-256)
- Store in a `HashMap<Hash, Vec<(FilePath, StartLine, EndLine)>>`
- Minimum duplicate length: ≥ 6 non-whitespace tokens (hard floor)
- Maximum window: 20 tokens (configurable)

### 5.3 Cluster and filter duplicates (`delve-core/src/duplicates.rs`)
- Collate hashes: if same hash appears in ≥ 2 locations, it's a duplicate
- Merge overlapping windows that form longer contiguous duplicates
- Filter out duplicates within the same function (allow across different files or distant parts of same file)
- For each cluster: pick the shortest representation, list all locations

### 5.4 Implement `dup` command
- Output duplicate clusters in human-readable format
- For each cluster show:
  - The duplicate snippet text (first occurrence)
  - All file:line-range locations
- `--json` flag for structured output

### 5.5 Tests for duplicate detection
- Test with exact duplicate blocks across two files
- Test with normalized duplicates (different variable names, different strings)
- Test that unique code is not reported
- Test with Jaccard-style near-duplicates (should not be caught by token-normalized approach)
- Test with very large files for performance

---

## Phase 6: Risk Pattern Detection

### 6.1 Detect `any` type usage (`delve-core/src/risks.rs`)
- AST query for `: any` type annotations (TypeScript only)
- AST query for `as any` type assertions
- Count per file, track file + line number of each occurrence
- Skip `.js` files (no TypeScript types)
- Output total count and per-file breakdown

### 6.2 Detect `console.log` / `debugger` statements (`delve-core/src/risks.rs`)
- Find `console.log`, `console.warn`, `console.error` call expressions — but count `console.log` only (warn/error are acceptable in prod)
- Actually, reconsider: the spec says `console.log` / `debugger` — count both `console.log` and `debugger` statements
- Skip files in `__tests__/`, `*.test.*`, `*.spec.*` directories (`console.log` is fine in tests)
- Report file + line number for each occurrence

### 6.3 Detect deep nesting (`delve-core/src/risks.rs`)
- Walk AST tracking nesting depth of `if`, `for`, `while`, `switch` blocks
- Any nesting > 4 levels → report
- Show file + line + actual depth
- Count how many such deeply nested blocks exist

### 6.4 Detect functions with too many parameters (`delve-core/src/risks.rs`)
- Walk function nodes, count formal parameters
- Threshold: > 5 parameters → report
- Show file + function name + line + parameter count

### 6.5 Tests for risk patterns
- Test `any` detection in `.ts` and `.tsx` files (not in `.js`)
- Test `console.log` detection (including in vs not in test files)
- Test `debugger` detection
- Test deep nesting detection at thresholds 3, 4, 5
- Test long parameter detection

---

## Phase 7: Health Score Calculation

### 7.1 Implement health score algorithm (`delve-core/src/health.rs`)
- Starting score: 100
- Default weights (from plan.md):
  - Unused exports: -15 per file that has any unused exports
  - Giant functions (critical): -5 each
  - Giant functions (warning): -2 each
  - Duplicate blocks: -3 each
  - `any` types: -1 each
  - `console.log` / `debugger`: -1 each
- Clamp to minimum 0
- Read configurable weights from `.delve.json` or CLI flags

### 7.2 Implement `health` command
- Run all analysis passes (unused, giant funcs, duplicates, risks)
- Calculate score
- Print score with rating label:
  - >= 70: "healthy" (green)
  - 40–69: "needs work" (yellow)
  - < 40: "vibe disaster" (red)
- Generate a prioritized TODO list:
  1. Fix unused exports (easiest, highest impact)
  2. Split giant functions
  3. Remove duplicate blocks
  4. Fix risky patterns
- `--json` flag outputs structured score + breakdown

### 7.3 Tests for health score
- Test score calculation with various combinations of issues
- Test with zero issues (should be 100)
- Test with extreme issues (should floor at 0)
- Test custom weights from config
- Test rating label boundaries (69, 70, 39, 40)

---

## Phase 8: Report Formatting & CLI

### 8.1 Build standard report formatter (`delve-core/src/report.rs`)
- Implement text formatter with section headers:
  - "UNUSED CODE (safe to delete)"
  - "GIANT FUNCTIONS (split me)"
  - "DUPLICATE BLOCKS"
  - "RISKY PATTERNS"
  - "HEALTH SCORE"
- Color code: red for critical, yellow for warning, green for healthy
- Use `yansi` for cross-platform ANSI colors
- Show file:line references for each finding

### 8.2 Build JSON report formatter (`delve-core/src/report.rs`)
- Serialize all findings to JSON
- Follow the schema from plan.md section 4
- Write to stdout when `--json` flag is passed
- Support piping to `jq` or other JSON processors

### 8.3 Implement `audit` command (main command, `delve-core/src/main.rs`)
- Run all analysis passes in sequence:
  1. File discovery
  2. Parse all files
  3. Build dependency graph
  4. Detect unused code
  5. Detect giant functions
  6. Detect duplicates
  7. Detect risk patterns
  8. Calculate health score
  9. Format and print report
- Accept `--json` flag
- Accept `--path` to specify target directory

### 8.4 Build CLI argument parser (`delve-core/src/main.rs`)
- Subcommands: `audit`, `deadcode`, `split`, `dup`, `health`
- Global flags: `--json`, `--path <path>`, `--config <path>`
- Per-command optional flags: `--threshold-warning`, `--threshold-critical`

### 8.5 Tests for report formatting
- Test text output format matches expected patterns
- Test JSON output is valid JSON
- Test with empty results (no issues found)
- Test that --json suppresses text output
- Test colored vs plain output

---

## Phase 9: npm Wrapper & Distribution

### 9.1 Build Rust binary (`npm run build` equivalent)
- `cargo build --release` produces the binary
- Create platform-specific npm packages:
  - `@glimpsecode/cli-darwin-x64`
  - `@glimpsecode/cli-darwin-arm64`
  - `@glimpsecode/cli-linux-x64`
  - `@glimpsecode/cli-linux-arm64`
  - `@glimpsecode/cli-win32-x64`
  - `@glimpsecode/cli-win32-arm64`

### 9.2 Write npm download script (`packages/cli/bin/delve.js`)
- On `postinstall`, detect platform/architecture
- Download the appropriate prebuilt binary from GitHub Releases
- Verify checksum (SHA-256)
- Place binary in `node_modules/.bin/delve`
- Fallback: if download fails, show error with manual install instructions
- Cache the binary to avoid re-downloading on repeated installs

### 9.3 Set up GitHub Actions release workflow
- On tag push (v*), trigger cross-compilation:
  - Linux: `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`
  - macOS: `x86_64-apple-darwin`, `aarch64-apple-darwin`
  - Windows: `x86_64-pc-windows-msvc`, `aarch64-pc-windows-msvc`
- Use `actions-rs/toolchain` or manual cross-compilation setup
- Upload artifacts to GitHub Release
- Add checksums file

### 9.4 Set up npm publishing
- `@glimpsecode/cli` is the main package
- Platform-specific packages are optional dependencies
- `package.json` structure:
  ```json
  {
    "name": "@glimpsecode/cli",
    "optionalDependencies": {
      "@glimpsecode/cli-darwin-x64": "0.1.0",
      "@glimpsecode/cli-darwin-arm64": "0.1.0",
      ...
    }
  }
  ```
- Publish to npm registry via `npm publish` in CI

### 9.5 Verify end-to-end flow
- `npm install -g @glimpsecode/cli`
- `glimpse audit --path ./some-project`
- Test on Windows, macOS, Linux
- Test with network failure (binary download fails → clear error)

---

## Phase 10: Configuration & Polish

### 10.1 Implement config file support (`.delve.json`)
- Read `.delve.json` from project root (or custom path via `--config`)
- Configurable fields:
  - `thresholds.warning_lines` (default: 40)
  - `thresholds.critical_lines` (default: 80)
  - `thresholds.warning_complexity` (default: 10)
  - `thresholds.critical_complexity` (default: 20)
  - `weights.unused_file` (default: 15)
  - `weights.giant_critical` (default: 5)
  - `weights.giant_warning` (default: 2)
  - `weights.duplicate` (default: 3)
  - `weights.any_type` (default: 1)
  - `weights.console_log` (default: 1)
  - `ignore` (array of glob patterns to skip)
  - `entry_points` (manual override for entry point files)
- Merge with CLI flags (CLI flags override config file)
- Validate config file and show helpful error on invalid fields

### 10.2 Add spinner/progress indicator
- Show spinner while analyzing (use `indicatif` crate)
- Show which phase is currently running: "Parsing files…", "Analyzing dependencies…", etc.
- Suppress spinner when `--json` is used or output is piped

### 10.3 Performance optimization
- Profile with a real-world codebase (e.g., a 10k+ line project)
- Target: < 1 second for small projects (< 100 files), < 5 seconds for medium projects (< 1000 files)
- Optimization strategies:
  - Parallel file parsing with `rayon`
  - Parallel duplicate detection with `rayon`
  - Early exit if no files found
  - Reuse parsed ASTs across analysis passes
  - Stream output instead of buffering everything

### 10.4 Error handling & edge cases
- Empty project (no source files found) → friendly message
- Project with only type definitions → no false unused positives on types
- Binary/symlink files → skip gracefully
- Permission errors → skip file with warning
- Very large files (> 5000 lines) → process but warn user
- Syntax errors → parse what we can, report partial results

---

## Phase 11: Documentation & Testing on Real Repos

### 11.1 Write README
- Badge section: build status, npm version, MIT license
- Quick start: `npx glimpse audit`
- Features overview (one paragraph each)
- Output examples (terminal + JSON)
- Installation: global vs npx
- CLI reference: all commands and flags
- Configuration: `.delve.json` reference
- CI integration example (GitHub Actions, `--json` flag)
- FAQ:
  - "Is this a linter?" — no, it's a complement to ESLint
  - "Will it delete my code?" — no (not yet)
  - "Can I use it on Python?" — not yet
- Contributing guide link

### 11.2 Dogfooding: Run Delve on Delve's own codebase
- Set up Delve as a pre-release check
- Fix any issues found in Delve's own code before shipping
- Document the dogfooding process

### 11.3 Test on 5+ real vibe-coded repos
- Find open-source projects that grew organically (hackathon projects, side projects)
- Run every command, verify output makes sense
- Collect feedback on false positives
- Adjust thresholds and heuristics based on findings
- Document learnings

### 11.4 Write CONTRIBUTING.md
- How to set up the development environment
- How to run tests
- How to add a new analysis pass
- Code style guide
- PR checklist
- Good first issues list

---

## Phase 12: Pre-release & Launch

### 12.1 Final QA pass
- Run full test suite
- Test on Windows (WSL or native)
- Test with Node.js 18, 20, 22
- Test with `npm`, `yarn`, `pnpm`
- Verify binary downloads work for all platforms

### 12.2 Create v0.1.0 release
- Tag commit `v0.1.0`
- GitHub Release with changelog and binary artifacts
- Publish to npm
- Post on social media / dev community

### 12.3 Post-launch issues
- Set up issue templates (bug report, feature request)
- Set up discussion board
- Monitor first-week feedback
- Prioritize bug fixes over new features

---

## Summary: Task Count by Phase

| Phase | Task Count |
|-------|-----------|
| 0. Scaffolding & Tooling | 5 |
| 1. Core Parsing & AST | 7 |
| 2. Import Resolution & Graph | 6 |
| 3. Unused Code Detection | 4 |
| 4. Giant Functions & Complexity | 5 |
| 5. Duplicate Code Detection | 5 |
| 6. Risk Pattern Detection | 5 |
| 7. Health Score Calculation | 3 |
| 8. Report Formatting & CLI | 5 |
| 9. npm Wrapper & Distribution | 5 |
| 10. Configuration & Polish | 4 |
| 11. Documentation & Testing | 4 |
| 12. Pre-release & Launch | 3 |
| **Total** | **61 tasks** |

Each task is designed to be independently implementable, testable, and reviewable. Tasks within a phase can often be parallelized (especially phases 1, 4, 5, 6).

---

# v2: Production Polish & Real-World Workflow

> **Theme**: Move from "it works on my machine" to "it works in your CI/CD".
> Focus: accuracy at scale, auto-fix, CI integration, deeper analysis.

---

## Phase 13: Smarter Duplicate Detection

### 13.1 Jaccard near-duplicate detection
- Beyond token-normalized exact matches: use Jaccard similarity on n-gram token sets
- Threshold: ≥ 0.7 similarity (configurable) → report as near-duplicate
- Helps catch copy-paste with variable renaming, whitespace changes, comment changes
- Merge near-duplicate clusters with exact-match clusters for unified reporting
- Implementation: sliding window over normalized tokens, compute set intersection/union

### 13.2 Structural duplicate detection (AST subtree hash)
- Hash the AST subtree structure (not tokens) to find functionally identical code with different naming/strings
- Example: two components that differ only in CSS class names and text content
- Use tree-sitter AST node kinds and relative positions, ignore identifier/string leaves
- Report: "Structurally identical to [file]:[line]"

### 13.3 Duplicate suppression & dedup
- `/* delve:no-dup */` comment to suppress per-block
- Configurable `--dup-min-lines` to ignore duplicates shorter than N lines (separate from token window)
- Cluster grouping: group duplicates by file pair for easier triage

---

## Phase 14: Import Resolution & Graph v2

### 14.1 Barrel file detection & resolution
- Detect barrel files (`index.ts` that only re-exports from other files)
- Follow re-exports transitively: `export { foo } from './bar'` → `foo` is reachable if the barrel is imported
- Mark barrel-file symbols as "re-export aliases" (don't report as unused even if the barrel itself seems unused)
- Handle `export * from './bar'` chains properly

### 14.2 Type-only import tracking
- Distinguish `import type { X }` and `import { type X }` from value imports
- Type-only exports consumed only by type imports should not be flagged as unused
- Track per-export: is it a type? is it consumed only as a type?
- Downrank type-only unused from "unused" to "info" in health score

### 14.3 Dynamic import analysis
- Statically analyze `import('./module')` strings when they're string literals
- Track dynamic imports as soft dependencies (mark as used, but flag as "dynamically loaded")
- Add a new report section: "DYNAMICALLY LOADED" for audit visibility

### 14.4 Circular dependency detection
- After graph build, find all strongly connected components (SCCs) with > 1 node
- For each SCC, list the cycle path: `A.ts → B.ts → C.ts → A.ts`
- Report section: "CIRCULAR DEPENDENCIES"
- Score penalty: -5 per circular chain (configurable weight)

### 14.5 Unused dependency detection (package.json)
- Scan project source for all import strings (npm package names)
- Cross-reference against `package.json` dependencies
- Report packages imported but not in `package.json` (missing dep) and packages in `package.json` but never imported (unused dep)

---

## Phase 15: Catastrophic Anti-Patterns

### 15.1 `any` propagation tracking
- Not just count `any` — trace `any` propagation: `function foo(x: any): any { ... }`
- If a function takes `any` and returns `any`, all callers also get `any`-tainted
- Report chains: "any flows from [file]:[line] → [file]:[line] → [file]:[line]"

### 15.2 Async/await misuse
- Detect promise chains that could be `async`/`await` (`.then().catch()` in async functions)
- Detect `await` in loops that could use `Promise.all`
- Detect forgotten `await`: `const x = someAsyncFn()` without `await` in an async function
- Detect floating promises (Promise returned but not awaited)

### 15.3 State management smells
- Detect `useState` + `useEffect` patterns that could be `useMemo` or derived state
- Detect `useState` that is set but never read
- Detect prop drilling > 3 levels deep (passing a prop through intermediate components)
- Detect `// @ts-ignore` / `// @ts-expect-error` usage count

### 15.4 Security & secrets
- Scan for hardcoded secrets: `password =`, `apiKey =`, `token = '...'` with string literals
- Detect `innerHTML` usage (XSS vector)
- Detect `eval()` or `new Function()` usage
- Flag `NODE_ENV` checks that might leak dev behavior in prod

---

## Phase 16: Auto-Fix & Interactive Mode

### 16.1 Basic `--fix` for unused code
- `glimpse deadcode --fix` — add `/* delve:used */` comment above each false positive
- `glimpse deadcode --fix --aggressive` — remove unused export statements (with git backup)
- Generate a diff file before making changes
- Dry-run mode: `glimpse deadcode --fix --dry-run` (show what would change, make no filesystem changes)

### 16.2 Auto-fix for console.log
- `glimpse audit --fix` — wrap each `console.log(...)` with `// eslint-disable-next-line no-console` or replace with `// GLIMPSE: remove console.log`
- Option to remove console.log entirely (except in test files)

### 16.3 Refactoring suggestions for giant functions
- For each giant function, suggest a split point:
  - Identify biggest code block within the function → suggest extracting it
  - Based on indentation depth or logical operator boundaries
  - Output as: "Consider extracting lines 42-67 into a `formatUserData()` function"
- Generate the suggested extracted function as a code snippet in the report

### 16.4 Interactive TUI mode
- `glimpse explore` — terminal UI using `ratatui` crate
- Navigate results with arrow keys
- Jump to file:line in editor (open $EDITOR)
- Filter by severity, file, kind
- Mark findings as ignored (writes `/* delve:used */` or similar)

---

## Phase 17: CI/CD Integration

### 17.1 SARIF output format
- Add `--sarif` flag to output Static Analysis Results Interchange Format (SARIF)
- Enables GitHub code scanning alerts, Azure DevOps, VS Code SARIF viewer
- Map each finding to SARIF result schema: level (error/warning/note), location, message
- Test: validate output against SARIF spec, verify GitHub imports it

### 17.2 GitHub Actions annotations
- In addition to SARIF, add `--github-annotations` flag
- Output `::warning file=foo.ts,line=42::message` format for GitHub Actions
- Each finding becomes an inline annotation on the PR diff
- Auto-detect `GITHUB_ACTIONS` env var and switch to annotation mode

### 17.3 Exit code semantics
- `glimpse audit`: exit 0 if health ≥ 70, exit 1 if health 40–69, exit 2 if health < 40
- Per-command: exit 0 if no findings, exit 1 if any findings
- `--fail-on <threshold>`: custom exit code threshold
- `--max-warnings <N>`: exit 1 if more than N findings (like ESLint)

### 17.4 HTML report
- `--html` flag generates a standalone HTML report
- Interactive: filterable table, sort by severity, search
- Summary dashboard: score gauge, finding counts, trend sparkline
- Embed all findings with file:line links (clickable in IDE via `file://` protocol)
- Dark mode toggle

---

## Phase 18: Performance & Scale

### 18.1 Incremental parsing & caching
- Cache parsed ASTs to disk (`.delve-cache/` directory)
- Use file modification timestamps + content hash to invalidate cache entries
- Only re-parse files that changed since last run
- Target: sub-second re-analysis for small changes

### 18.2 Large project optimizations
- Lazy file loading: only parse files when they're first needed by the graph
- Streaming output: flush results per file instead of buffering everything
- Memory-bounded: process in batches of 500 files, flush intermediates
- Graceful degradation: warn if project > 10,000 files, offer `--quick` mode (skip duplicates & graph)

### 18.3 Parallel I/O for file discovery
- Use multiple threads for filesystem walking (especially on slow filesystems like network drives)
- Batch `stat()` calls across threads
- Respect `fs.inotify.max_user_watches` limits (handle gracefully, don't crash)

### 18.4 Benchmark suite
- Create a `bench/` directory with large synthetic codebases
- Measure: parse time, graph build, duplicate detection
- Assert: no regressions > 10% from baseline
- CI: run benchmarks on every PR, comment with perf diff

---

## Phase 19: Plugin System & Extensibility

### 19.1 Custom rule API (Rust-side)
- Define a `Rule` trait: `fn check(&self, ctx: &RuleContext) -> Vec<Finding>`
- Built-in rules: unused exports, giant funcs, duplicates, risks
- Users can write custom rules in Rust and register them via config: `plugins = ["./my-rules.dll"]`
- Config-driven: `rules.my_custom_rule.enabled = true`, `rules.my_custom_rule.severity = "error"`

### 19.2 Custom rule API (JS-side)
- Allow JavaScript-based custom rules via a small DSL
- Load `.delve/rules/*.js` files, each exports a `check(ctx)` function
- Provide a `Finding` constructor and a `walkAST(node, callback)` helper
- Example: `module.exports.check = (ctx) => { ... }`

### 19.3 Config inheritance & presets
- `.delve.json` extends from a base config: `"extends": ["recommended", "./my-base.json"]`
- Built-in presets: `"recommended"` (default), `"strict"`, `"relaxed"`
- Per-preset weight and threshold overrides
- `glimpse init` command: interactive `.delve.json` generator

---

## Phase 20: Advanced Report UX

### 20.1 Trend tracking & history
- Save health scores to `.delve-history.json` (timestamp + score + finding counts)
- `glimpse trend` command: show score over time (last 7 days, 30 days, all)
- ASCII sparkline in terminal
- `--json` includes historical data for dashboard integration

### 20.2 Diff analysis (`glimpse diff`)
- `glimpse diff --base main` — run analysis only on changed files vs a git ref
- `glimpse diff --base v0.1.0` — compare current state to a tag
- Health delta: "Score went from 85 → 72 (bad)" or "72 → 85 (good)"
- Useful in CI: gate PRs based on health score change
### 20.3 Summary mode (`glimpse summary`)

- `glimpse summary` — condensed single-line output: "health: 85, issues: 12 (3 critical, 9 warning)"
- Perfect for commit messages, Slack webhooks, status badges
- `glimpse summary --badge` — generate a shields.io-style SVG badge

### 20.4 Machine-readable output enhancements
- `--ndjson` flag: newline-delimited JSON (stream one finding per line)
- `--github-issue` flag: format as a GitHub issue body (markdown sections with code blocks)
- `--slack` flag: format as Slack message blocks

---

## Summary: v2 Task Count

| Phase | Task Count | Theme |
|-------|-----------|-------|
| 13. Smarter Duplicates | 3 | Near-duplicate, structural, dedup |
| 14. Import Resolution v2 | 5 | Barrel files, type-only, dynamic, circular, deps |
| 15. Catastrophic Anti-Patterns | 4 | any propagation, async, state, secrets |
| 16. Auto-Fix & Interactive | 4 | --fix, giant function split, TUI |
| 17. CI/CD Integration | 4 | SARIF, GH annotations, exit codes, HTML |
| 18. Performance & Scale | 4 | Incremental, lazy, parallel, benchmark |
| 19. Plugin System | 3 | Rust rules, JS rules, config presets |
| 20. Report UX | 4 | Trends, diff, summary, machine output |
| **Total** | **31 tasks** | |

## Roadmap Priority

**P0 (v2.0.0, next)**
- 14.1 Barrel file detection
- 14.2 Type-only import tracking
- 14.4 Circular dependency detection
- 17.1 SARIF output
- 17.2 GitHub Actions annotations
- 17.3 Exit code semantics

**P1 (v2.1.0)**
- 13.1 Jaccard near-duplicate
- 14.5 Unused dependency detection
- 16.1 Basic --fix for unused code
- 18.1 Incremental parsing

**P2 (v2.2.0)**
- 15.1–15.4 Anti-patterns
- 17.4 HTML report
- 18.4 Benchmark suite
- 20.1 Trend tracking

**P3 (v2.3.0+)**
- 19.1–19.3 Plugin system
- 16.4 Interactive TUI
- 20.2–20.4 Advanced output
- 18.2 Large project optimizations
