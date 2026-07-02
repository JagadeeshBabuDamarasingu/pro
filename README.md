# pro

A secure-by-default AI productivity suite: one shared Rust core engine,
exposed through a CLI today and a GUI/TUI/cloud target later, with
capability-scoped sandboxing, an audit trail, and built-in defenses against
prompt injection, supply-chain hallucination, and low-confidence output —
treated as core-engine responsibilities, not per-feature afterthoughts.

Full product spec: [`.pro/specs/`](.pro/specs/) (start at
[`00-vision.md`](.pro/specs/00-vision.md)). Implementation status and
per-crate design notes: [`AGENTS.md`](AGENTS.md). This README covers how to
build, configure, and run what exists today.

**Status**: Phase 0 (foundations) and Phase 2 (security-core hardening) are
built. No TUI or GUI yet — those are Phase 1. See
[`.pro/specs/09-roadmap.md`](.pro/specs/09-roadmap.md) for the full phase
plan and [`AGENTS.md`](AGENTS.md) for exactly what's implemented vs.
deliberately deferred in each crate.

## Prerequisites

- Rust (stable toolchain — `cargo`, `rustc`)
- Node.js + [pnpm](https://pnpm.io/) (for the turbo-orchestrated workspace;
  not required to build/run the Rust core directly)
- An OpenAI-compatible model endpoint — [Ollama](https://ollama.com/) is the
  easiest local default (`ollama serve`, then pull a model), or any hosted
  provider that exposes a `/v1/chat/completions`-shaped API + key

## Setup

```bash
pnpm install
```

## Configuration

The Rust core reads `pro.config.toml` from the current directory by default
(override with `--config <path>` on any command). Minimal example:

```toml
[provider]
name = "ollama"
base_url = "http://localhost:11434/v1"
api_key_env = ""            # name of an env var holding the API key, or "" for none
model = "functiongemma:latest"

[profile]
default = "read-only"        # read-only | workspace | danger-full-access

[audit]
log_path = ".pro/audit.jsonl"
retention_days = 90

# Optional — enables `pro ask --cascade`: a strong tier only called when the
# cheap/local [provider] tier's own answers disagree with each other.
[cascade_provider]
name = "..."
base_url = "https://api.example.com/v1"
api_key_env = "EXAMPLE_API_KEY"
model = "..."
```

`pro.config.mjs` at the repo root is reserved for future JS-facing surfaces
(Phase 1+) and is not read by the Rust core — don't confuse the two.

Every capability a profile grants (filesystem, network, process execution)
starts at autonomy tier 0 (shadow/observe-only) by design — nothing runs
unsandboxed by default. See [`AGENTS.md`](AGENTS.md) and
[`.pro/specs/02-security-core.md`](.pro/specs/02-security-core.md) for the
full capability/sandbox model.

## Building

```bash
# via turbo (drives the whole Rust workspace as the "pro-engine" package)
pnpm exec turbo run build
pnpm exec turbo run test
pnpm exec turbo run lint

# equivalent, direct cargo
cd packages/core
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

The `pro` binary lives at `packages/core/target/debug/pro` after a build, or
run it directly with `cargo run --manifest-path packages/core/Cargo.toml -p
pro-cli --`. Run commands from the repo root so the default
`pro.config.toml` resolves correctly.

## Usage

```
pro [OPTIONS] <COMMAND>

Commands:
  config        Inspect the resolved project configuration
  grant         Inspect the active capability grant inventory
  ask           Send a prompt to the configured provider end-to-end
  slop          Score a unified diff against the AI Slop Index
  supply-chain  Run the supply-chain gate against a package

Global options:
  --config <PATH>   Path to the project config file [default: pro.config.toml]
  --json            Emit machine-readable JSON on stdout instead of plain text
  --quiet           Suppress non-essential stderr diagnostics
  --no-input        Never block on interactive input (also respects CI=true)
```

`pro` is dual-mode by design: every command works identically whether run
interactively or from CI (`--json` for structured output, `NO_COLOR`
respected, distinct non-zero exit codes per failure class — 1 config, 2
capability-denied, 3 provider, 4 sandbox, 5 usage, 6 supply-chain-rejected).

### `pro config show` / `pro grant list`

```bash
pro config show
pro grant list          # every active capability grant + its autonomy tier
```

### `pro ask` — talk to the configured provider

```bash
pro ask "explain this error message"

# Confidence-gated: samples the provider N times, clusters by meaning, and
# reports semantic entropy alongside the answer instead of silently
# presenting a possibly-confabulated response.
pro ask --confidence --samples 5 "..."

# Model cascade: tries the cheap/local [provider] tier first, escalates to
# [cascade_provider] only if the cheap tier's own samples disagree.
pro ask --cascade --entropy-threshold 0.3 "..."
```

### `pro slop` — AI Slop Index on a diff

Deterministic heuristics (duplication ratio, scope discipline) run for
free; `--full` adds LLM-judge axes (behavioral correctness, test
correctness, mechanical cleanliness, convention conformance), which costs
an API call.

```bash
git diff | pro slop -
pro slop path/to.diff --full --scope src/auth --scope src/billing
```

### `pro supply-chain check` — typosquat / hallucinated-package gate

Run this before trusting an AI-suggested dependency add. Checks existence
first (catches pure hallucination), then typosquat/edit-distance against
known-popular packages cross-checked with real download counts, install-
script static analysis (npm), and publish-velocity.

```bash
pro supply-chain check npm left-pad        # Clear
pro supply-chain check npm reqwests        # likely Suspicious or NonexistentPackage
pro supply-chain check crates serde --json
```

## Repository layout

```
pro/
  .pro/specs/            product spec — read before changing architecture
  .pro/research/         grounded investigations behind non-obvious design calls
  AGENTS.md              implementation status, per-crate design notes
  pro.config.toml        example/default core-engine config
  pro.config.mjs         reserved for future JS-facing surfaces (not yet used)
  packages/
    core/                the Rust workspace ("pro-engine" to turbo)
      crates/
        pro-config/          config loading
        pro-capability/      capability/grant policy layer
        pro-sandbox/         sandbox enforcement layer
        pro-audit/           audit log
        pro-provider/        OpenAI-compatible provider client
        pro-provenance/      taint tracking / injection defense
        pro-supply-chain/    typosquat / hallucinated-package gate
        pro-confidence/      confidence signals + model cascade
        pro-slop/            AI Slop Index
        pro-core/            Engine tying everything together
        pro-cli/             the `pro` binary
    cli/                 empty — Phase 1: thin npm wrapper over the Rust binary
    gui/                 empty — Phase 1: Tauri shell
    web/                 empty — not yet scoped
```

## License

Apache-2.0 — see [`LICENSE`](LICENSE).
