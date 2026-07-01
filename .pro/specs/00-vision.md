# pro — vision

## One-liner

A secure-by-default AI productivity suite — one shared engine, two
human-facing surfaces (GUI, CLI/TUI) plus non-interactive execution modes
(CI, cloud), with pluggable "audience packs" that give developers,
builders, security researchers, designers, DevOps engineers, analysts, and
others a coherent, safety-first AI layer over the tools they already use.

## The problem

Every current AI dev-tool is single-surface and single-audience: GUI-only
tools (Cursor, Windsurf, Zed, Replit, Devin) have no real CLI/CI story;
CLI/TUI-only tools (Aider, Claude Code, Amp, opencode) have no GUI for
less-technical users. Security posture is bimodal — a couple of tools
(Claude Code, Warp) enforce real OS-level sandboxing, most others (Cursor,
Aider, Continue.dev, Amp) rely on pattern-matched allowlists or plain user
trust, and Cursor's allowlist model has already been defeated by a real CVE.
Nobody outside developers is treated as a first-class persona — security
researchers, designers, analysts, and DevOps engineers all get "developer
tools with different branding" rather than a workflow built around how they
actually work.

## Product pillars

1. **One core, every surface and execution target.** A shared Rust core
   engine owns orchestration, tool execution, and security enforcement. The
   GUI (Tauri) and the CLI/TUI (Rust, wrapped for npm distribution) are both
   thin clients over the same core; CI invocation and cloud/background
   execution ([12-cloud-task-execution.md](12-cloud-task-execution.md)) are
   non-interactive modes of that same core, not separate products with their
   own trust model — a task looks and behaves identically whether started by
   a non-technical user clicking a button, a developer running a TUI
   command, a CI pipeline invoking `--json --no-input`, or an unattended
   cloud session. No competitor currently spans this much surface area with
   one engine.
2. **Secure by default, Deno-style.** No filesystem, network, subprocess, or
   env access without an explicit, scoped capability grant — enforced at the
   OS level (not just pattern-matched), with provenance-aware elevation
   prompts and an audit log. See [02-security-core.md](02-security-core.md).
3. **Built-in defenses against AI failure modes**, not bolted on: prompt
   injection, typosquatting/slopsquatting, hallucination, and "AI slop" are
   treated as core-engine responsibilities every audience pack inherits for
   free, not a feature any one pack has to reimplement.
4. **Audience packs, not one generic chatbot.** Each audience gets narrow,
   verifiable, auditable capabilities suited to their actual workflow (a
   root-cause diagnostic agent for DevOps, not a general chatbot; a
   scope-gated pentest extension for security researchers; a design-to-code
   fidelity pipeline for designers) rather than one undifferentiated agent
   with a different skin.
5. **Provider-neutral, local-first optional.** One model-provider abstraction
   (OpenAI-compatible endpoint + key) covers hosted providers (Anthropic,
   OpenAI, Gemini) and local inference (Ollama, llama.cpp, vLLM)
   interchangeably — swapping models is a config change, never a code change.
6. **Cost-aware by design, never at the expense of correctness.** Prompt
   caching, model cascading, and a persistent codebase/doc knowledge graph
   ("graphify") cut token spend and latency as a cross-cutting core-engine
   concern — but every optimization sits upstream of the capability/sandbox
   system and the verification pipeline, never in place of them. See
   [10-cost-optimization.md](10-cost-optimization.md).

## Audiences

| Audience | Core need | Primary surface |
|---|---|---|
| Developers / builders | Codegen, agent orchestration, parallel work, code review | CLI/TUI, GUI |
| Startups (small teams) | All of the above, cross-role, low ops overhead | GUI + CLI |
| Security researchers | AI-assisted pentesting, safely gated | CLI/TUI |
| Designers | AI + design tooling, Figma-depth integration | GUI |
| DevOps engineers | Deployment, observability, incident response | CLI/TUI |
| Analysts | NL-to-SQL, BI, reporting | CLI + GUI |
| Technical writers | Grounded doc generation from real specs/code | GUI + CLI |
| QA / test engineers | NL-to-test generation, self-healing tests, triage | CLI/TUI |
| Product managers | Feedback synthesis, PRD drafting, roadmap analysis | GUI |
| SREs* | Root-cause correlation, on-call load reduction | CLI/TUI (DevOps-adjacent) |
| Legal / contract-review | Citation-grounding, defined-term/cross-reference verification | GUI |
| Data / ML engineers | Lineage-grounded pipeline edits, schema/backfill safety | CLI/TUI |
| Customer support engineers | Policy-grounded answers, bounded refund/credit autonomy | GUI + CLI |
| Open-source maintainers | AI-slop-scored triage of external contributions | CLI/TUI |

Full detail per audience lives in `03`–`08` and `13`–`16`. Two rows above
are not independent packs:

- **Startups (small teams)** is not a distinct feature pack — it's a
  cross-role bundling of the other packs (a developer wearing the DevOps
  and analyst hats too), relevant to packaging/pricing rather than to the
  spec surface.
- **SREs*** — [08-audience-additional.md](08-audience-additional.md) frames
  merge-vs-separate against the DevOps pack as an open product decision,
  leaning toward merging unless SRE-specific demand proves otherwise; the
  table row above reflects the default (lean-merge) outcome, not a settled
  split.

No-code / "citizen developer" users are explicitly **out of scope** —
"pro" targets technical and semi-technical roles operating through a
CLI/TUI or a real GUI, not a Bolt/v0-style zero-code app builder.

## Non-goals (for now)

- Not a hosted SaaS-only product — the core engine and CLI must work fully
  local/self-hosted/air-gapped.
- Not trying to out-generate the design-tool market (Figma Make, v0, Uizard
  already do this) — the designer pack's edge is integration depth and
  code-fidelity, not a better generator.
- Not shipping offensive-security capability without an explicit
  authorization gate — see [04-audience-security-researchers.md](04-audience-security-researchers.md).
