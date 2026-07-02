# AGENTS.md — pro

Working notes for AI agents (and humans) picking up this repo. Full product
spec lives in `.pro/specs/` (read `.pro/specs/README.md` first, then
`00-vision.md` → `09-roadmap.md`). This file tracks *implementation*
progress against that spec — the specs describe intent, this file describes
what actually exists in code right now.

## Repo layout

```
pro/
  .pro/specs/            product spec (17 files) — read before changing architecture
  .pro/research/         grounded investigations behind non-obvious implementation calls
  package.json            root: pnpm + turbo orchestration
  pnpm-workspace.yaml      packages: ["packages/*"]
  turbo.json               tasks: build, test, lint
  pro.config.mjs           reserved for future JS-facing surfaces (untouched, not yet used)
  pro.config.toml          actual config the Rust core reads (+ optional [cascade_provider])
  packages/
    core/                  turbo package "pro-engine" — wraps the Rust workspace
      Cargo.toml           Cargo workspace manifest
      package.json         turbo scripts: build/test/lint -> cargo build/test/clippy --workspace
      crates/
        pro-config/        TOML config schema + loader
        pro-capability/    capability/grant policy layer (see below)
        pro-sandbox/       SandboxEnforcer trait + enforcement layer (see below)
        pro-audit/         append-only JSONL audit log
        pro-provider/      OpenAI-compatible provider client (hosted + Ollama)
        pro-provenance/    taint tracking, quarantine, MELON divergence primitive (Phase 2)
        pro-supply-chain/  npm/crates.io existence, typosquat, install-script gate (Phase 2)
        pro-confidence/    sampling-consistency, semantic entropy, model cascade (Phase 2)
        pro-slop/          AI Slop Index diff scorer (Phase 2)
        pro-core/          Engine: ties every other crate together
        pro-cli/           `pro` binary (clap)
    cli/                   empty — Phase 1: thin TS/npm wrapper over the Rust binary
    gui/                   empty — Phase 1: Tauri shell
    web/                   empty — not yet scoped
```

## Build / test / run

```bash
# via turbo (drives the whole "pro-engine" package)
pnpm exec turbo run build
pnpm exec turbo run test
pnpm exec turbo run lint

# direct cargo (equivalent, faster iteration)
cd packages/core
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings

# CLI, run from repo root (reads ./pro.config.toml by default)
cargo run --manifest-path packages/core/Cargo.toml -p pro-cli -- config show
cargo run --manifest-path packages/core/Cargo.toml -p pro-cli -- grant list
cargo run --manifest-path packages/core/Cargo.toml -p pro-cli -- ask "hello"
cargo run --manifest-path packages/core/Cargo.toml -p pro-cli -- ask --confidence "hello"
cargo run --manifest-path packages/core/Cargo.toml -p pro-cli -- supply-chain check npm left-pad
git diff | cargo run --manifest-path packages/core/Cargo.toml -p pro-cli -- slop -
```

Local dev provider: Ollama at `http://localhost:11434/v1` (already running in
this environment with `functiongemma:latest` — see `pro.config.toml`).

## Status: Phase 0 (foundations) — done

Per `.pro/specs/09-roadmap.md`, Phase 0 is the mandatory dependency root.
Implemented, tested (32 unit tests passing), clippy-clean:

- **`pro-config`** — `Config::load` parses `pro.config.toml`. Deliberate
  divergence from the spec's `.mjs` filename: no Node-shellout reader was
  built, to avoid a Node runtime dependency in the Rust core during this
  phase. Revisit for Phase 1 npm packaging if `.mjs` support becomes a hard
  requirement.
- **`pro-capability`** — scoped `Capability` variants (`FsRead`/`FsWrite`/
  `Net`/`Run`/`Env`, never a bare boolean); `Grant` bound to a
  **content hash of the source definition** (not a path — closes the
  documented "approve once, exploit forever" flaw); `AutonomyTier` 0–4 with
  every capability defaulting to tier 0 (shadow); `AutonomyMetadata::
  autonomy_ceiling()` caps tier by reversibility/blast-radius/signal-quality;
  `GrantStore` with explicit-deny-always-wins merge logic and stale-grant
  detection; default profiles (`read-only`/`workspace`/`danger-full-access`).
  `RunBinary::validate_argv` does argument-shape validation against granted
  read/write scopes (parsed argv, not regex).
- **`pro-sandbox`** — `SandboxEnforcer` trait. **Only `ShadowEnforcer` is
  implemented** (records intent, never executes — correct behavior for
  tier-0, not a placeholder). Promoting a `run` capability past tier 0 with
  no real backend registered is a hard `SandboxError`, never a silent
  unsandboxed fallback. **No Linux (bubblewrap/Landlock), macOS (Seatbelt),
  or Windows (AppContainer) backend exists yet** — this is the biggest
  Phase 0 gap and the most likely next real infrastructure investment.
- **`pro-audit`** — append-only local JSONL log (`.pro/audit.jsonl`,
  gitignored); `AuditEvent` has the three-way `causal_class`
  (`Hallucination`/`Injection`/`PrivilegeDrift`, populated at incident-review
  time, not by default) plus `reasoning_summary`/`evidence_chain`;
  `RetentionPolicy` and a `redact()` stub exist but redaction patterns
  aren't wired to any real secret/PII detector yet.
- **`pro-provider`** — one `OpenAiCompatibleClient` serves hosted providers
  and Ollama identically. `Segment::cache_hint` exists for a future
  Anthropic-style explicit `cache_control` client; the current client only
  relies on stable-prefix ordering (works for OpenAI/Ollama-style automatic
  prefix caching, does nothing extra yet).
- **`pro-core`** — `Engine` is the single choke point: every `ToolAdapter`
  method (`fs_read`/`fs_write`/`run`/`net_fetch`/`env_get`) is
  capability-check → (sandbox, for `run`) → audit-log → side effect, always
  in that order. `PromptAssembler` orders segments stability-first.
- **`pro-cli`** — `pro config show` / `pro grant list` / `pro ask <prompt>`.
  Dual-mode: TTY detection + `--no-input`/`CI=true`, `--json`, `--quiet`,
  distinct exit codes per failure class (1=config, 2=capability-denied,
  3=provider, 4=sandbox, 5=usage). No TUI, no GUI — that's Phase 1.

**Verified live**: `pro ask` round-tripped through the local Ollama instance
and the exchange was correctly written to `.pro/audit.jsonl`.

Phase 1 (TUI, npm wrapper, Tauri GUI shell) was intentionally **not** built
yet — by user direction, work moved to Phase 2 next. Per the roadmap's
sequencing principle, phases after 0 are reorderable, so this is within
spec ("every phase after 0 is optional and reorderable").

## Status: Phase 2 (security-core hardening) — done

Per `.pro/specs/09-roadmap.md` Phase 2 and `.pro/specs/02-security-core.md`
§2–4. Implemented, tested (78 unit tests passing workspace-wide),
clippy-clean, verified live against real registries/the local Ollama
instance:

- **`pro-provenance`** — `Provenance` taint lattice (`UserDirect` <
  `ToolOutput` < `Untrusted`, `combine()` picks the least-trusted);
  `require_user_direct()` is the CaMeL-lite enforcement point (a function,
  not a model self-check) — exported but **not yet wired into any
  `ToolAdapter` call site**, since doing so would mean changing those
  method signatures, which was out of this pass's approved scope.
  `Quarantine::summarize` always tags its output `Untrusted` regardless of
  what the quarantine model itself claims. MELON: `action_divergence` +
  `is_suspected_injection` (low divergence between a baseline and a
  masked-instruction re-run is the injection signal — *not* high
  divergence) are real and tested; `Engine::check_injection_risk` wires
  them in but **returns `None` until a caller can supply both trajectory
  arms**, which needs Phase 1's agent loop. Spotlighting lives in
  `pro-core`'s `PromptAssembler::assemble_with_untrusted`.
- **`pro-supply-chain`** — `RegistryClient` for npm (`registry.npmjs.org` +
  `api.npmjs.org` downloads) and crates.io, both live-network. Existence
  check → typosquat/edit-distance against a small embedded popular-package
  list, cross-checked with real download counts → install-script heuristic
  scan (npm only — crates.io's `install_scripts` deliberately returns
  empty; see the doc comment on `CratesIoRegistryClient` for why) →
  publish-velocity flag. `sonatype-guide` (this environment's MCP tool) is
  explicitly **not** wrapped here — that's an agent-level integration
  concern, not runtime code in the `pro` binary.
- **`pro-confidence`** — see `.pro/research/confidence-signals.md` for the
  full investigation. Three real signals: `LexicalConsistency` (SelfCheckGPT-
  style, free), `EntailmentClustering` (full Semantic Entropy via judge
  calls — the strong signal), `logprob_confidence` (opportunistic, `None`
  when the backend doesn't return logprobs). **Deliberately does not
  implement Semantic Entropy *Probes*** — that variant needs hidden-state
  access no HTTP provider abstraction can expose; treated as an
  architectural ceiling, not a gap to close later. `Cascade` escalates
  cheap→strong tier on low confidence; `Engine::ask_cascaded` /
  `Engine::ask_with_confidence` wire it into the engine; `[cascade_provider]`
  is an optional second block in `pro.config.toml`.
- **`pro-slop`** — hand-rolled unified-diff parser; deterministic
  `duplication_ratio`/`scope_discipline` heuristics (no API cost); FrontierCode-
  axis LLM-judge scoring (`--full`) tagged by CodeHalu category. **Not**
  auto-wired into `Engine`/any codegen path — there isn't one yet (Phase 1)
  — exposed standalone via `pro slop`.
- **`pro-cli` additions** — `pro ask --confidence|--cascade [--samples N
  --temperature T --entropy-threshold X]`, `pro slop <file|-> [--full]
  [--scope PATH...]`, `pro supply-chain check <npm|crates> <package>`. New
  exit code: 6 = supply-chain-rejected.

**Verified live**: `pro supply-chain check` against real npm (`left-pad` →
Clear, a nonexistent package → `NonexistentPackage`, `reactjs` → `Suspicious`/
`TyposquatCandidate` against real download counts) and crates.io (`serde` →
Clear); `pro slop` against a synthetic diff correctly flagged both a
duplicated line and an out-of-scope file touch; `pro ask --confidence` and
`pro ask --cascade` (escalation path included) both round-tripped through
the local Ollama instance with audit events logged.

Not done in this pass, by explicit scope decision (see the plan history):
`require_user_direct` isn't called from any `ToolAdapter` method yet;
`check_injection_risk` has no real caller yet (no trajectory loop exists);
Semantic Entropy Probes were ruled out, not deferred.

## Conventions worth knowing before touching this code

- Everything capability-gated must go through `pro_core::Engine`'s
  check-then-log path — don't add a second ad hoc capability check anywhere.
- Never make `run` execution silently bypass `EnforcerRegistry` — if a tier
  needs a real backend and none exists, that must stay a hard error.
- New crates should stay narrow (one concern each) per the existing split,
  not grow `pro-core` into a monolith.
- `cargo clippy --workspace --all-targets -- -D warnings` must stay clean;
  fix lints rather than allow them unless there's a documented reason.
