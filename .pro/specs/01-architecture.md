# pro — architecture

## Competitive landscape (2025-2026 snapshot)

| Tool | Audience | GUI/CLI split | Security posture |
|---|---|---|---|
| Cursor | Individual devs | GUI only | Allowlist-based; defeated by CVE-2026-22708 (shell-builtin sandbox bypass) |
| Windsurf | Autonomous-agent devs | GUI only | Ephemeral per-session sandbox, default-deny egress allowlist |
| Zed | Perf-focused devs | Native GUI only | Speed/privacy-first, no distinct sandbox architecture |
| Warp | Devs + DevOps | Terminal GUI + cloud agents | Per-run Docker isolation, SOC2, ZDR |
| GitHub Copilot CLI | Broad GitHub users | CLI only (Workspace GUI discontinued May 2025) | Sandbox toggle exists (can be disabled) |
| Sourcegraph Cody | Enterprise | GUI/IDE only | SOC2, admin model allowlist, no execution sandbox |
| Continue.dev | Privacy-conscious teams | IDE extension | No built-in sandbox — DIY |
| Aider | Terminal-first individuals | CLI only | No sandbox |
| opencode | Provider-neutral terminal devs | TUI (Go+Bubble Tea) over TS/Bun backend | Plan/build mode split; no OS-level sandbox |
| Amp | Long-horizon codebase agents | CLI-first | Not well documented publicly |
| Claude Code | Devs, solo→CI | CLI/TUI only | OS-level sandbox (bubblewrap/Seatbelt), 5 permission modes, hooks |
| Devin | Enterprise "delegate the ticket" | Cloud GUI only | Sandboxed cloud env |
| Replit Agent | Beginners/indie hackers | Browser GUI only | Isolated hosted infra |

**Gaps this project targets:**

1. No tool spans GUI → CLI/TUI → CI with one engine and identical guarantees.
2. Security/sandboxing maturity is bimodal; "pro" ships OS-enforced
   sandboxing as a cross-cutting core feature, not a CLI-only add-on.
3. Nobody treats security researchers, designers, analysts, or DevOps as
   first-class personas with dedicated, narrow, verifiable capabilities.
4. CI/scripting parity is bolted on late almost everywhere; "pro" designs
   dual-mode (interactive/CI) into the core engine from day one.
5. Provider-neutral orchestration + a real GUI is unclaimed (opencode owns
   "any-model, terminal-only"; Devin/Replit own "one model, cloud-GUI-only").

## Engine, surfaces, and execution targets

```
                    ┌─────────────────────────────┐
                    │   Shared Rust core engine    │
                    │  orchestration · tools ·     │
                    │  capability/sandbox system   │
                    │  provider abstraction ·      │
                    │  audit log · verification    │
                    │  knowledge graph (graphify)  │
                    │  · prompt cache · cascade    │
                    └──────────────┬──────────────┘
           ┌───────────────┬───────┴───────┬───────────────┐
           │               │               │               │
    ┌──────▼──────┐ ┌──────▼──────┐ ┌──────▼──────┐ ┌──────▼──────┐
    │ GUI (Tauri)  │ │CLI/TUI(Rust)│ │ CI invoke   │ │Cloud target │
    │ desktop app  │ │clap+ratatui │ │ same binary,│ │ same core + │
    │ + embedded   │ │+ thin TS/npm│ │ --json      │ │ credential  │
    │ terminal     │ │  wrapper    │ │ --no-input  │ │ broker      │
    └─────────────┘ └─────────────┘ └─────────────┘ └─────────────┘
```

CI invocation and cloud execution are both non-interactive **modes/targets**
of the same core, not separate UI surfaces — a human interacts with "pro"
through the GUI or the CLI/TUI; CI and cloud are how a task runs when no
human is present at the moment of execution. Cloud specifically is a fourth
execution target the same policy/audit substrate governs, not a
separately-trusted surface — see
[12-cloud-task-execution.md](12-cloud-task-execution.md).

The core engine is the only place business logic and security enforcement
live. Every surface is a client of it — no surface reimplements
orchestration, sandboxing, or provider selection independently.

## GUI: Tauri

**Choice: Tauri**, not Electron, not Flutter, not native-per-platform.

- The Rust core engine links directly into the Tauri app as a command layer —
  no IPC/serialization tax, no duplicated business logic (this is the
  deciding factor: it directly serves the shared-core design goal).
- Capability-based permission manifest is a natural fit for the
  security-forward positioning (auditable, declarative OS-access grants vs.
  Electron's full-Node + full-Chromium surface).
- Proven pattern for embedding a real terminal/TUI panel: `tauri-plugin-pty`
  + xterm.js (used by real 2026 projects; same approach VS Code itself uses
  with Electron+node-pty). This lets the GUI offer a "drop to terminal" panel
  without leaving the app — mirrors how opencode's own Tauri desktop app is
  built.
- Smaller install (~8MB vs Electron's ~165MB), lower idle memory (~45MB vs
  ~180MB) — matters for a "productivity" pitch.
- Trade-offs to plan for: WebView-per-OS rendering quirks (WebView2 / WKWebView
  / WebKitGTK) need QA budget; smaller Rust+web talent pool than pure-web;
  mobile support is real (v2) but newer/less battle-tested than Flutter's — if
  first-class native mobile becomes a near-term priority, revisit.

## CLI/TUI: Rust core, thin TS/npm wrapper

**Choice:** primary binary in Rust (`clap` for subcommands, `ratatui` —
targeting v2 once stable, expected ~March 2026 — for the interactive view),
distributed to the npm/JS ecosystem via a thin wrapper, following the
esbuild/SWC/Biome/Turborepo playbook (native binary + `napi-rs` bindings or a
postinstall binary fetch, no reimplementation in TypeScript).

- Rust gives CI-relevant instant cold start (no Node/Bun runtime required) —
  a single dependency-free binary.
- `citty` (unjs) is the preferred thin JS-facing router if a plugin
  architecture isn't needed yet; `oclif` if third-party npm-based plugins
  become a requirement (Rust has no native equivalent to oclif's plugin
  model — build that layer only if/when needed).
- Interaction design modeled on the best-regarded dev-tool TUIs: lazygit's
  persistent panel layout and reversible-by-default destructive actions,
  gitui/bottom's native-rendering speed, k9s's narrow-and-excellent scope,
  gh CLI's composability (`--json`, `-q` field selection, stable exit codes,
  man-page-quality `--help`).
- **Dual-mode is designed in, not retrofitted**: detect TTY + an explicit
  `--no-input`/`CI=true` convention; never block on a prompt in a
  non-interactive context. Machine mode gets `--json` (NDJSON event-stream
  for long-running tasks), `--quiet`, `NO_COLOR` support, stdout for
  results/stderr for diagnostics, and stable non-zero exit codes per failure
  class — following the Command Line Interface Guidelines (clig.dev).
- Security/sandboxing enforcement lives entirely in the Rust core, so the
  TUI, the GUI, and any CI invocation all inherit the same OS-level
  guarantees rather than each surface reimplementing its own (weaker)
  permission model.

## Provider abstraction

One provider abstraction — base URL + API key, OpenAI-compatible surface —
covers hosted providers (Anthropic, OpenAI, Gemini, Bedrock, Groq,
OpenRouter) and local inference (Ollama by default for dev ergonomics,
llama.cpp for offline/no-GPU, vLLM for self-hosted concurrent serving)
identically. Swapping models/engines is a config change. This mirrors what
kubectl-ai and Spacelift Intelligence already do and is required by the
DevOps, Builder, and Analyst packs alike (see `03`, `06`, `07`).

## Orchestration model

Two orchestration shapes, chosen per task rather than one-size-fits-all:

- **Declarative/graph orchestration** (LangGraph-style state graph, or a
  Conductor-style declarative YAML workflow) for repeatable, auditable
  engineering tasks — deploys, codegen pipelines, CI-invoked work. Control
  flow should be inspectable and reproducible, not hidden inside prompts.
- **Conversational/open-ended orchestration** (AutoGen/CrewAI-style
  agent-to-agent loops) reserved for genuinely exploratory tasks (research
  subagents, brainstorming) that can't be predefined upfront.

Default to the first shape; only use the second where the task is
irreducibly open-ended.

## Cost optimization is a core-engine concern

Prompt caching, model cascading, static/semantic knowledge-graph context
retrieval, and local-model efficiency all live in the shared core so every
surface inherits them identically — see
[10-cost-optimization.md](10-cost-optimization.md) for the full design and
priority order. As with security, no surface should reimplement its own
(weaker, inconsistent) version of these.
