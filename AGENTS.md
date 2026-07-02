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
        pro-repomap/       tree-sitter repo map: symbol graph + PageRank (Phase 1)
        pro-lsp/           minimal LSP client (Phase 1)
        pro-grounding/     LSP-backed + AST/symbol-table grounding gate (Phase 1)
        pro-worktree/      git worktree lifecycle + best-of-N runner (Phase 1)
        pro-review/        simple/adversarial/orthogonal code review (Phase 1)
        pro-orchestrate/   declarative step-graph vocabulary + executor (Phase 1)
        pro-core/          Engine: ties every other crate together
        pro-cli/           `pro` binary (clap)
    cli/                   empty — Phase 1 (deferred slice): thin TS/npm wrapper over the Rust binary
    gui/                   empty — Phase 1 (deferred slice): Tauri shell
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
cargo run --manifest-path packages/core/Cargo.toml -p pro-cli -- repomap
cargo run --manifest-path packages/core/Cargo.toml -p pro-cli -- ground path/to/file.rs
cargo run --manifest-path packages/core/Cargo.toml -p pro-cli -- run "some task" --parallel 3 --verify "cargo build" --file src/lib.rs
```

Local dev provider: Ollama at `http://localhost:11434/v1` (already running in
this environment with `functiongemma:latest` — see `pro.config.toml`).
`pro ground` additionally shells out to `rust-analyzer` for Rust files
(confirmed present in this environment: `rust-analyzer 1.96.0`).

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

Phase 1's TUI/npm-wrapper/Tauri-GUI slice was intentionally **not** built
yet — by user direction, work moved to Phase 2 next, then to Phase 1's
codegen/orchestration slice (see below). Per the roadmap's sequencing
principle, phases after 0 are reorderable, so this is within spec ("every
phase after 0 is optional and reorderable").

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

## Status: Phase 1, codegen/orchestration slice — done (TUI/GUI/npm wrapper deferred)

Per `.pro/specs/09-roadmap.md` Phase 1, scoped to the codegen/orchestration
core only (no new UI surface). Implemented, tested (132 unit tests passing
workspace-wide), clippy-clean, verified live against the real codebase, a
real `rust-analyzer` session, and a real scratch git repo:

- **`pro-repomap`** — Aider-style repo map: tree-sitter over Rust + TS/JS
  (node names verified against the real grammars via scratch S-expression
  dumps before writing queries, not guessed), files-as-nodes /
  symbol-reference-weighted-edges graph, hand-rolled PageRank (petgraph has
  no built-in implementation), `SymbolIndex` for name→defining-file lookup.
  Name-based, not scope-resolved — first-definer-wins on name collisions is
  a real, documented precision limit.
- **`pro-lsp`** — minimal hand-rolled LSP client (`lsp-types` for message
  structs, hand-rolled stdio `Content-Length` framing, `tokio` read-loop
  dispatching by request id / forwarding `publishDiagnostics`).
  `LspError::ServerNotFound` when the binary isn't on `PATH` — never hangs.
- **`pro-grounding`** — two-tier grounding gate. Rust gets a real
  rust-analyzer-backed `textDocument/definition` check; **a real bug was
  caught during live verification here**: rust-analyzer returns
  `Some(Array([]))` for "no definition found," not `None` — treating any
  `Some(_)` as resolved silently passed every hallucinated call. Fixed
  (`definition_response_is_empty`) and regression-tested. TS/JS and
  Rust-without-rust-analyzer use the AST/symbol-table fallback via
  `pro-repomap`'s `SymbolIndex` — documented false-positive rate on
  external/stdlib calls (no semantic type info at that tier). Also flags
  nonexistent referenced file paths (string-literal heuristic, no `regex`
  dependency).
- **`pro-worktree`** — real `git worktree add/remove/list` (shells out, no
  git library dependency, consistent with the rest of the codebase);
  `run_best_of_n` creates N worktrees, runs a `Proposer` in each, scores by
  a real verifier command's exit code, tears down every non-winner
  (worktree **and** branch). Live-verified against a real scratch git repo.
- **`pro-review`** — `simple` (one pass), `adversarial` (real CLAIM→EXTRACT→
  DOUBT→RECONCILE as four sequential calls building one conversation, not
  four independent prompts), `orthogonal` (two independently-configured
  providers, zero shared context, Jaccard-similarity disagreement flag —
  reuses the `[cascade_provider]` config slot as "the other model").
- **`pro-orchestrate`** — fixed Propose/Ground/Verify/Review step
  vocabulary (not a generic plugin system), `petgraph`-backed topological
  ordering, fail-fast execution, `serde_yaml` round-trip for a declarative
  form. The actual side effects live behind its `StepExecutor` trait — this
  crate owns vocabulary/ordering only.
- **`pro-core` wiring** — `Engine::propose_diff` (capability-gated
  `fs_read` for context, structured `{files: [{path, new_content}]}`
  output, diffed against disk via `similar` — never hand-authored diff
  syntax from the model). `WorkflowRunner` implements `StepExecutor`,
  composing repomap/grounding/worktree-pattern/review; its `Verify` step
  goes through the existing tiered `Engine::run` (capability-gated, and
  **inert at tier 0** since no real sandbox backend exists yet — same
  honesty pattern as Phase 0, not a new gap). `pro-worktree`'s own verifier
  is a separate, already-real code path (shells out directly inside an
  already-isolated worktree, no tier involved) — that's the one that
  actually executes in this pass.
- **`pro-cli` additions** — `pro repomap [--budget N]`, `pro ground <path>`,
  `pro review [--adversarial|--orthogonal] <diff-file>`, `pro run <task>
  --parallel N --verify "<cmd>" [--file PATH...]`. New exit codes:
  7=grounding-failed, 8=review-disagreement, 9=no-best-of-n-winner.

**Verified live**: `pro repomap` against `packages/core` itself (correctly
ranks `pro-core/src/lib.rs`, the most cross-referenced file, highest);
`pro ground` against both a clean and a hallucinated-call fixture through a
real `rust-analyzer` session (this is where the empty-array bug was caught
and fixed); `pro review` round-tripped through local Ollama; `pro run
--parallel 2 --verify "true"` against a real scratch git repo confirmed both
worktrees were created, the passing candidate survived with its branch
intact, and the losing worktree + branch were both fully torn down.

Not done in this pass, by explicit scope decision: the TUI (`ratatui`), the
npm distribution wrapper, the Tauri GUI shell, and the time-ledger/session-
memory/cost-estimation layer from `03-audience-developers-builders.md` —
all deferred to a later Phase 1 slice. `WorkflowRunner`'s 4-step pipeline
has no dedicated CLI command in this pass (the plan scoped the CLI to four
independent commands instead); it's real, tested, and ready to be exposed
once there's a concrete reason to run the full chain as one command rather
than its parts separately.

## Status: Linux sandbox backend (bubblewrap + Landlock) — done

Per `.pro/specs/09-roadmap.md` Phase 0 and `.pro/specs/02-security-core.md`
§1. Fills what AGENTS.md previously called "the biggest Phase 0 gap":
`pro-sandbox` had only `ShadowEnforcer` (records intent, never executes).
Implemented, tested (25 new `pro-sandbox` unit tests + 5 in the new
`pro-sandbox-helper` binary, workspace total 144 tests, all passing),
clippy-clean, verified live in this exact WSL2 6.6 environment:

- **`pro_sandbox::linux::enforcer::LinuxBwrapEnforcer`** (Linux-only,
  `#[cfg(target_os = "linux")]`) — real `SandboxEnforcer` backend. Builds
  an explicit `bwrap --unshare-all` argv (never a blanket `--ro-bind / /`):
  a minimal host environment (`/usr`, `/bin`, `/lib`, `/lib64`,
  `/etc/resolv.conf`, `/etc/ssl`, filtered to what actually exists on the
  host), the caller's `RunSpec.read_paths`/`write_paths` identity-bound
  read-only/read-write, and a small helper binary + scratch status file
  bound at dedicated in-sandbox paths. `LinuxBwrapEnforcer::new()`
  verifies both `bwrap` and the helper binary are actually present at
  construction time (hard error, never deferred to first use — mirrors
  `pro_lsp::LspError::ServerNotFound`'s "map the spawn error, don't
  which-preflight" pattern).
- **`pro_sandbox::landlock_status::LandlockEnforcement`** — honest,
  four-state record (`FullyEnforced`/`PartiallyEnforced`/`NotEnforced`/
  `HelperFailedBeforeAttempt`) of whether the additional Landlock
  path-restriction layer actually applied, carried on every
  `EnforcementOutcome::Executed`. Lives at the crate root (not under the
  Linux-only module) since it's pure data on a platform-agnostic type —
  the crate still compiles on non-Linux targets even though only the
  Linux backend constructs this variant today.
- **`pro-sandbox-helper`** (new `[[bin]]` in the `pro-sandbox` package,
  itself `#[cfg(target_os = "linux")]`-gated internally so the package
  still compiles elsewhere) — runs *inside* the bwrap sandbox, builds a
  Landlock ruleset (via the `landlock` crate, ABI V3, `CompatLevel::
  BestEffort`) scoped to exactly the granted paths plus the base host
  binds and the status file itself (both needed post-`restrict_self()`),
  records the real `RulesetStatus` to the status file, then
  `std::os::unix::process::CommandExt::exec()`s into the real target —
  process-image replacement is what makes the installed Landlock
  restriction (inherited across `execve`) actually govern the sandboxed
  command.
- **Corrected assumption from planning**: a missing
  `/sys/kernel/security/landlock` securityfs entry was initially taken as
  proof this WSL2 kernel lacks Landlock support. That's wrong — Landlock
  doesn't expose a securityfs entry at all (unlike some other LSMs). Live
  testing proved the opposite: with zero Landlock rules granted, even
  `/bin/echo` fails to exec with `EPERM`; granting the base host binds as
  Landlock rules makes it succeed again — that behavior is only possible
  if this kernel genuinely enforces Landlock (confirmed: ABI V3). This let
  the `FullyEnforced` branch be live-verified directly in this
  environment, not just the degraded `NotEnforced` path originally
  expected.
- **`pro_core::ToolAdapter::run`** now also audit-logs the execution
  outcome (exit code, stdout/stderr, Landlock status) after a successful
  `enforce()` — previously only the allow/deny decision was logged, not
  what actually happened once execution was attempted.
- **`pro-cli` addition**: `pro sandbox-smoke --ro PATH... --rw PATH...
  --env NAME... -- <cmd> [args...]` — a raw enforcer smoke-test that
  constructs a `RunSpec` directly and calls a `LinuxBwrapEnforcer`,
  bypassing capability/grant/tier checks entirely (and the audit log).
  Exists because no CLI command reaches `ToolAdapter::run` through the
  normal path yet (see gaps below); documented as a debug surface only,
  not "how you run sandboxed commands for real." New exit code: reuses
  `EXIT_SANDBOX_ERROR` (4) directly, since this command bypasses `Engine`.

**Verified live**: `pro sandbox-smoke --ro /tmp --rw /tmp -- /bin/echo hi`
round-tripped through real bubblewrap with `landlock=FullyEnforced { abi: 3
}`; reads/writes inside granted paths succeed and reads/writes outside
them fail with real namespace-level absence (not just permission errors);
network fully blocked when no hosts are requested (real `connect()`
attempt via `python3` inside the sandbox); a nonexistent `bwrap`/helper
path correctly hard-errors with a typed `SandboxError` rather than
silently degrading.

Not done in this pass, by explicit scope decision: **per-host network
filtering** — `RunSpec.net_hosts` non-empty is a hard
`SandboxError::PerHostNetworkFilteringUnsupported`, never silently
full-open or full-blocked; bubblewrap alone can only fully block or fully
share the host netns, and per-host egress filtering needs a separate
netns+nftables/proxy layer. **Seccomp syscall filtering** — namespace/
mount isolation (bwrap) + Landlock only this pass. **Tier-promotion
workflow** — `Grant::new_shadow` still always sets tier 0/Shadow; there is
still no API or CLI command anywhere in the workspace to promote a grant,
so `EnforcerRegistry` can still never route to a real backend through the
normal `Engine`/CLI path — this is why `pro sandbox-smoke` exists as a
bypass rather than wiring `LinuxBwrapEnforcer` into `pro-cli`'s existing
`EnforcerRegistry::new()` call sites (`cmd_ask`, `CodegenProposer::
propose`), which remain untouched. **macOS (Seatbelt) / Windows
(AppContainer) backends** — not started.

## Conventions worth knowing before touching this code

- Everything capability-gated must go through `pro_core::Engine`'s
  check-then-log path — don't add a second ad hoc capability check anywhere.
- Never make `run` execution silently bypass `EnforcerRegistry` — if a tier
  needs a real backend and none exists, that must stay a hard error.
- New crates should stay narrow (one concern each) per the existing split,
  not grow `pro-core` into a monolith.
- `cargo clippy --workspace --all-targets -- -D warnings` must stay clean;
  fix lints rather than allow them unless there's a documented reason.
- Don't use `/sys/kernel/security/landlock` (or any securityfs path) as a
  signal for whether Landlock is supported — it doesn't expose one, unlike
  some other LSMs. The only reliable check is attempting a real ruleset
  and reading back its actual `RulesetStatus`/`LandlockStatus`, as
  `pro-sandbox-helper::apply_landlock` does.
