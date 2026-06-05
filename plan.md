## Project Delve – Precise Plan for TS/JS (Vibe Coder Edition)

### Core Promise
> One command (`npx delve audit`) gives you a friendly, actionable report of code quality problems typical in vibe‑coded projects: **unused code, giant functions, duplicates, inconsistent naming, and risky patterns** – with zero config and sub‑second speed.

### Target Audience
- Solo developers, hackathon participants, AI‑assisted coders
- Projects that grew organically without strict linting
- Anyone who wants to clean up before sharing code or deploying to production

---

## 1. Commands (MVP)

| Command | What it does |
|---------|---------------|
| `npx delve audit` | Full report (unused, big functions, duplicates, complexity hotspots, risky patterns) |
| `npx delve deadcode` | Focus only on unused exports / functions |
| `npx delve split` | Suggest where to break large functions (list lines + complexity) |
| `npx delve dup` | Show duplicate code blocks with file/line references |
| `npx delve health` | Print a single health score (0–100) and a short todo list |

All commands accept `--json` for CI/editor integration.

---

## 2. Technical Architecture

```
[npm package]                 [Rust binary]
@ronak-jain-afk/cli      ──spawn──▶   delve-core
    │                              │
    └─── postinstall         ┌─────┴─────┐
         downloads binary    │ tree-sitter│
                             │ rayon     │
                             │ serde_json│
                             └───────────┘
```

- **Rust crate** (`delve-core`): Parsing, analysis, JSON report generation.
- **npm wrapper** (`delve`): Downloads prebuilt binary for platform, runs it, prints output.
- **Language support**: TypeScript + JavaScript (`.ts`, `.tsx`, `.js`, `.jsx`, `.mjs`, `.cjs`).
- **Build & distribution**: `cargo build --release`, GitHub Actions cross‑compile to Linux/macOS/Windows, upload to npm as optional dependency.

---

## 3. Feature Specification (Evidence‑Based, No AI)

### 3.1 Unused Code Detection
**Goal**: Find exports / top‑level functions that are never referenced.

**How it works**:
- Parse all source files, build a graph of **exported symbols** → **imported symbols**.
- Entry points heuristics (in order):
  1. `package.json` → `main`, `module`, `bin`, `exports`
  2. Files named `index.ts`, `main.ts`, `cli.ts`, `app.ts`
  3. Any file containing `if (require.main === module)` or `if (import.meta.url === ...)`
- Traverse from entry points, mark reachable exports.
- **Report**: Exported symbols with zero inbound references.
- **Caveat**: Mark as “definitely unused” – dynamic imports (`import()`), `eval`, and global usage are ignored (documented). User can add `/* delve:used */` comment to silence.

### 3.2 Giant / Complex Functions
**Metrics**:
- **Lines of code** (excluding comments and blank lines)
- **Cyclomatic complexity** (using tree‑sitter’s control flow nodes: `if`, `for`, `while`, `&&`, `||`, `?.`, `??` etc.)

**Thresholds** (configurable via `.delve.json` or CLI flags, but default works for vibe coders):
- `warning`: > 40 lines **or** complexity > 10
- `critical`: > 80 lines **or** complexity > 20

**Output**: File + function name + line range + metric values.

### 3.3 Duplicate Code Blocks
**Method**:
- Token‑based fingerprinting (normalized: ignore whitespace and rename identifiers `a1`, `a2`…).
- Slide a window of 5–20 tokens, hash each window, collate identical hashes across files.
- Report duplicates of **≥ 6 non‑whitespace tokens** that appear in at least two locations.

**Output**: For each duplicate cluster, show the smallest snippet and list all file:line locations.

### 3.4 Risk Patterns (Small but High‑Impact)
- `any` type usage (TypeScript only) – count per file
- `console.log` / `debugger` statements left in non‑test files
- Extremely deep nesting (> 4 levels of `if`/`for` inside each other)
- Functions with > 5 parameters

### 3.5 Health Score (0–100)
Weighted sum (default weights, user can override):
- Unused exports count (‑15 per file that has any)
- Giant functions count (‑5 per critical, ‑2 per warning)
- Duplicate blocks count (‑3 each)
- `any` types count (‑1 each)
- `console.log` count (‑1 each)

Starting score = 100, subtract until floor 0. Score >= 70 = “healthy”, 40–69 = “needs work”, <40 = “vibe disaster”.

---

## 4. Output Examples

### Terminal (Human)
```
Delve Audit – my-project

UNUSED CODE (safe to delete)
  src/utils/formatDate.ts:3   formatTimestamp (exported, never imported)
  src/hooks/useScroll.ts:1    useScroll (function defined, never called)

GIANT FUNCTIONS (split me)
  src/components/Dashboard.tsx:45-112   renderDashboard (68 lines, complexity 15)
  src/api/client.ts:22-74               fetchWithRetry (52 lines, complexity 12)

DUPLICATE BLOCKS
  src/helpers/validateEmail.ts (L23-31)  duplicates  src/lib/checkEmail.ts (L10-18)

RISKY PATTERNS
  src/types/api.ts:12   any type used (avoid!)
  src/index.ts:17       console.log left in production

HEALTH SCORE: 42/100 – “needs work”
  → Run `npx delve deadcode --remove` to delete unused exports (dry‑run first)
```

### JSON (for tools)
```json
{
  "score": 42,
  "unused": [{"file":"src/utils.ts","symbol":"oldHelper","line":7}],
  "giantFunctions": [{"file":"src/app.ts","name":"process","startLine":45,"endLine":112,"lines":68,"complexity":15}],
  "duplicates": [{"fingerprint":"abc123","locations":["file1:23-31","file2:10-18"]}],
  "risks": {"anyTypes":3,"consoleLogs":2,"deepNesting":1,"longParams":0}
}
```

---

## 5. Implementation Roadmap (6 weeks, part‑time)

### Week 1 – Foundation
- [ ] Rust project setup (`tree-sitter`, `rayon`, `serde`)
- [ ] Parse a single TS/JS file, extract functions, exports, imports
- [ ] Build naive symbol table per file

### Week 2 – Graph & Unused Detection
- [ ] Resolve imports across files (relative, package‑internal)
- [ ] Build global export graph
- [ ] Identify entry points heuristics
- [ ] Generate unused report (without false positives for dynamic patterns)

### Week 3 – Giant Functions & Complexity
- [ ] Walk AST to count lines (minus comments/blanks) per function
- [ ] Compute cyclomatic complexity (visit `if`, `for`, `while`, `&&`, `||`, etc.)
- [ ] Output warnings/criticals

### Week 4 – Duplicate Detection
- [ ] Tokenize source, normalize identifiers
- [ ] Rolling hash + dictionary for exact duplicate blocks
- [ ] Output clustered duplicates

### Week 5 – Risk Patterns & Health Score
- [ ] Detect `any`, `console.log`, nesting depth, param count
- [ ] Calculate weighted health score
- [ ] Add `--json` flag for all commands

### Week 6 – npm Wrapper, CLI Polish, Docs
- [ ] Rust binary builds (GitHub Actions cross‑compile)
- [ ] npm package with postinstall download script
- [ ] Colored terminal output (use `yansi` or ANSI codes)
- [ ] Write README: examples, installation, configuration (`.delve.json`)
- [ ] Test on 5 real vibe‑coded repos (your own + open source)

---

## 6. Open Source & Student Project Tips

- **License**: MIT (invites contributions)
- **Repository name**: `delve` on GitHub (check availability)
- **First release**: v0.1.0 – only `audit` and `deadcode` commands, rest as experimental flags
- **Contribution guide**: “Help wanted: implement duplicate detection for Vue SFC” etc.
- **Dogfood**: Use Delve to clean up Delve’s own codebase before each release

---

## 7. What to **Not** Do (Save for v2)

- No cross‑file duplicate semantic analysis (too complex)
- No automatic refactoring (risky)
- No language server / LSP integration
- No incremental / watch mode
- No git integration (“what changed” can be `git diff | delve` later)

---


