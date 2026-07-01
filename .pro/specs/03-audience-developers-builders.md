# pro — developers & builders

## Code generation & agent orchestration

Default to declarative/graph orchestration (LangGraph-style or Conductor-style
YAML) for repeatable engineering tasks; reserve conversational orchestration
(CrewAI/AutoGen-style) for genuinely open-ended work — see
[01-architecture.md](01-architecture.md#orchestration-model). All model
access goes through the shared provider abstraction so a task can run against
Anthropic/OpenAI/Gemini or a local model (Ollama default, llama.cpp offline,
vLLM for self-hosted concurrent serving) by config change alone.

## Trust and measured productivity (validated pain points)

Two survey findings anchor this section — see
[11-user-research-pain-points.md §1](11-user-research-pain-points.md) for
full evidence: Stack Overflow 2025 finds developers' #1 complaint (66%) is
"almost right, but not quite" output, with trust falling even as adoption
rises; the METR study found experienced developers on real tasks in mature
repos were **actually 19% slower** using an AI coding assistant while
believing they were **20% faster** — a 39-point perception gap driven by
verification overhead the developer doesn't consciously track.

- **Task-level time ledger**: the Rust core separately instruments
  generation time, verification/review time, and rework time per task, and
  reports the actual net delta rather than letting perception substitute
  for measurement — this makes "pro" the only tool with an evidence-based,
  not vibes-based, productivity claim, and it's cheap given the core
  already instruments every step for the audit log ([02-security-core.md](02-security-core.md)).
- **Auto-detected low-fit contexts**: use the verification pipeline's own
  hit/miss rate (per repo, per language) to detect niche-language or
  huge-monorepo contexts where AI assistance is empirically unreliable, and
  downgrade agent autonomy there rather than assuming uniform competence
  everywhere.
- **Persistent, structured session memory**: auto-generate and version a
  machine-readable decision log (what was decided, why, what was rejected)
  at each meaningful checkpoint, loaded back deterministically at session
  start — this turns the community's manual CLAUDE.md-style workaround
  (documented as needed because agents lose track of decisions after ~45
  minutes) into a built-in, non-optional core-engine feature.
- **Pre-flight cost estimate with a hard budget cap**: before executing a
  multi-step task, estimate token/compute cost from the planned steps and
  require explicit confirmation above a user-configurable threshold — this
  is the direct fix for the Cursor pricing-surprise incident (a flat-fee
  product silently rebilled at raw API rates, producing bills like "$350 in
  a week" with no warning); see
  [10-cost-optimization.md §7](10-cost-optimization.md) for the mechanism.

## Code review: simple, adversarial, orthogonal

See [02-security-core.md §4](02-security-core.md) for the full rationale
(Osmani's finding that heterogeneous reviewers catch disjoint bugs that
homogeneous multi-sampling misses). This is exposed to builders as three
review modes on any diff:

- `pro review` (simple) — one fast pass, default for everyday commits.
- `pro review --adversarial` — CLAIM → EXTRACT → DOUBT → RECONCILE → STOP,
  in-flight on a decision before it's finalized; use before merging
  security-sensitive or irreversible changes.
- `pro review --orthogonal` — routes the diff through a genuinely different
  model/tool with no shared context; surfaces disagreement for a human call
  rather than auto-resolving.

A formal study of AI-authored PRs found **61.38% receive no recorded human
review activity at all**, and of comments that do occur, 71.58% come from
other AI review bots rather than humans reviewing substance — a measured
over-trust pattern, not a hypothetical one. "pro" pushes back on this
directly rather than leaving it to reviewer discipline:

- **Small, single-concern diffs by default** — the agent splits a large
  logical change into a sequence of reviewable commits/PRs rather than
  producing one large diff, the single most-cited community mitigation for
  review fatigue.
- **Disclosed per-hunk confidence in the PR description itself** — which
  lines were verified against compiler/types/tests vs. best-effort/
  unverified, plus an explicit list of assumptions made — so low-confidence
  code can't be silently rubber-stamped because "the AI probably got it
  right."
- **Reviewer-load-aware throttling** — track a team's review-queue depth
  and slow or batch the agent's own PR output when the queue is saturated,
  rather than treating review capacity as an externality.
- **Non-strippable AI-authorship labeling** plus a per-PR risk score
  (defect-likelihood from complexity/verification coverage), addressing the
  documented "can't tell if it's AI-authored, and that ambiguity itself
  erodes trust" pattern directly.

## Parallel work: git worktrees + best-of-N

Git worktrees are now standard infrastructure for parallel AI coding agents:
each agent gets its own linked working directory + branch off one shared
`.git` object database, enabling true parallel, non-conflicting execution.
Reference implementations worth studying: **Conductor** (dashboard,
diff-first review UI), **Vibe Kanban** (open-source, kanban board over
worktrees, MCP-driven task decomposition, 10+ agent backends), **Claude
Squad** (terminal-first, tmux + worktrees, closest fit if this stays
CLI/TUI-only). Reported practical sweet spot: 3-5 parallel agents before
worktree overhead outweighs benefit.

**Best-of-N pattern**: spin up N worktrees, run the same task at varied
temperature/seed in each, score candidates with an automated verifier
(compiler/type-check/lint/test suite — not an LLM-judge alone wherever a
real verifier exists), auto-select or present a diff comparison, tear down
losers. This is a natural `pro` primitive:
`pro run --parallel N --verify <test-cmd>`.

This same worktree-as-isolated-execution-context pattern is the scheduling
unit for cloud task execution too — `pro run --target cloud --parallel N`
runs each worktree as an unattended cloud session instead of a local one,
reusing the identical verifier gate and review UI. See
[12-cloud-task-execution.md](12-cloud-task-execution.md) for the full
design (modeled on, and hardening gaps found in, Claude Code cloud
sessions, Cursor Background Agents, GitHub Copilot's coding agent, and
Devin).

The same worktree isolation is also the execution sandbox open-source
maintainers use to safely run *external, untrusted* contributions before
a human reviews them — see
[16-audience-oss-maintainers.md](16-audience-oss-maintainers.md).

## Local model usage

One provider abstraction, OpenAI-compatible endpoint + key, covers:

| Engine | Fit |
|---|---|
| Ollama | Default local-dev path — simplest CLI ergonomics |
| llama.cpp | Offline/air-gapped, consumer/no-GPU hardware |
| LM Studio | GUI-managed local models, reference for the GUI's own bundled model manager |
| vLLM | Self-hosted concurrent serving at scale (continuous batching, far higher throughput) |

Swapping engines is a config change, never a code change — this is required
symmetrically by the DevOps and Analyst packs too
([06](06-audience-devops.md), [07](07-audience-analysts.md)).

## Figma / design-to-code for builders

Builders need code-fidelity more than generation — see
[05-audience-designers.md](05-audience-designers.md) for the full design-tool
survey. The relevant piece for this audience: integrate via **Figma's Dev
Mode MCP server** (structured hierarchy/tokens/component data direct to any
MCP client) with **Code Connect** (maps Figma components to the actual
code file/path so generated code references the real component library
instead of inventing markup) — modeled on Builder.io Visual Copilot's
"map to existing components" approach rather than parallel/duplicate
component generation.
