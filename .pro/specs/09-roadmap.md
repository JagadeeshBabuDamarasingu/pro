# pro — roadmap

Phased so that each phase only depends on what's already built. Security core
(§1–2 of [02-security-core.md](02-security-core.md)) is the dependency root —
nothing else should ship ahead of it.

## Phase 0 — Foundations

- Shared Rust core engine skeleton: orchestration primitives, tool-execution
  interface, config loading (`pro.config.mjs`).
- Capability & sandbox system: policy layer (scoped grants) + OS-level
  enforcement layer (bubblewrap/Landlock, Seatbelt, AppContainer), default
  profiles (`read-only`/`workspace`/`danger-full-access`), audit log.
- Content-hash-scoped grants (not path-scoped), per-capability autonomy
  tiers defaulting to shadow mode, blast-radius bounds on production-write
  capabilities, and the three-way audit causal classification
  (hallucination/injection/privilege-drift) — all validated by documented
  real-world incidents, not theoretical hardening
  ([02-security-core.md §1](02-security-core.md),
  [11-user-research-pain-points.md](11-user-research-pain-points.md)). Build
  these into the Phase 0 sandbox from the start; retrofitting content-hash
  scoping onto a path-scoped model later is exactly the flaw the research
  identifies in current competitors.
- Provider abstraction (OpenAI-compatible endpoint + key), wired to at least
  one hosted provider and Ollama.
- Minimal CLI (Rust, `clap`) — no TUI yet, no GUI yet. Dual-mode
  (interactive/CI) designed in from the first command.
- Stability-ordered prompt assembly + provider prompt caching, and
  structured-output-by-default for internal agent steps
  ([10-cost-optimization.md](10-cost-optimization.md) §1, §7) — build these
  alongside the capability system since every later phase's prompts flow
  through this same assembly path.

## Phase 1 — Developer/builder MVP

- `ratatui` TUI on top of the Phase 0 CLI.
- Thin TS/npm distribution wrapper (citty or oclif + prebuilt binary).
- Codegen + declarative-graph orchestration for repeatable tasks.
- Code review modes: simple, then adversarial, then orthogonal
  ([03](03-audience-developers-builders.md)).
- Git-worktree parallel execution + best-of-N with a real verifier
  (compiler/test suite).
- Tauri GUI shell with embedded terminal panel (`tauri-plugin-pty` +
  xterm.js), thin client over the same Rust core.
- Static dependency-graph context (Aider-style repo map via `tree-sitter`)
  as the default codebase-context mechanism
  ([10-cost-optimization.md](10-cost-optimization.md) §4) — zero-LLM-cost,
  and the first rung of the knowledge-graph ladder built out in Phase 3.
- Deterministic AST/LSP symbol-grounding gate ahead of the compiler/linter
  stage ([02-security-core.md §4](02-security-core.md)) — cheap, near-100%
  precision, catches hallucinated APIs/imports/paths before they're shown.
- Task-level time ledger, persistent structured session memory, small-diff-
  by-default with disclosed per-hunk confidence, and pre-flight cost
  estimate with a hard budget cap
  ([03-audience-developers-builders.md](03-audience-developers-builders.md)) —
  these directly answer the METR productivity-perception gap and the
  Cursor pricing-surprise incident, so they belong in the MVP, not deferred
  polish.

## Phase 2 — Security-core hardening

- Provenance/injection-resistant context assembly: quarantine pattern for
  content ingestion, spotlighting enforced in the prompt-assembly layer,
  CaMeL-style taint tracking, MELON-style masked re-execution before
  high-stakes actions.
- Supply-chain gate: existence check, edit-distance/download-disparity
  check, install-script warning, `sonatype-guide` integration.
- Confidence signals before presenting output: semantic-entropy probes
  where hidden states are available, SelfCheckGPT-style sampling
  consistency as the black-box-API fallback; low confidence routes to
  escalation/orthogonal review rather than silent display.
- Model cascade using the verification pipeline's pass/fail as the
  escalation signal ([10-cost-optimization.md §3](10-cost-optimization.md)) —
  ships alongside the confidence-signal work above since they share the
  same escalation mechanism.
- AI Slop Index as an automatic pre-review quality gate, using FrontierCode's
  mergeability axes and GitClear's baselines for calibration, tagged by the
  CodeHalu hallucination taxonomy.

## Phase 3 — DevOps + Analyst packs

- DevOps: read-only correlation layer over existing tools (K8s, cloud,
  Grafana/Prometheus, Datadog, PagerDuty, GitHub), Steampipe-style SQL
  abstraction, typed triage-action menu, non-prod-free/prod-gated
  provisioning split.
- Baseline-exclusion config for anomaly training data, the verification-tax
  metric (time-to-suggest vs. time-to-verify per capability), and the
  structured-evidence-chain RCA UI that suppresses low-confidence
  hypotheses by default ([06-audience-devops.md](06-audience-devops.md)) —
  these ship with the correlation layer itself, not as follow-on polish,
  since they directly answer documented alert-fatigue and RCA-trust
  failures.
- Analyst: NL-to-SQL with a governed metrics layer, scriptable/scheduled
  reports, GUI notebook/chart view.
- Metric-provenance logging and SelfCheckGPT-style resampling for NL-to-SQL
  ([07-audience-analysts.md](07-audience-analysts.md)) — directly answers
  the documented Power BI Copilot silent-wrong-answer incident; ship
  alongside the governed metrics layer, not after it.

## Phase 4 — Designer + security-researcher packs (gated)

- Designer: Figma Dev Mode MCP + Code Connect integration, REST API sync
  (comments/annotations/library), design-to-code fidelity pipeline.
- Reference-grounded generation (no blank-prompt path), the component/token
  grounding gate, and scope-discipline diffing on canvas writes
  ([05-audience-designers.md](05-audience-designers.md)) ship as part of
  the same Figma integration work, not bolted on after — these are
  guardrails on the generation path itself, not a separate feature.
- Security researcher: offensive-security capability tier — scope
  declaration + network-layer scope enforcement + authorization gate +
  immutable audit log, adapters for Nmap/Nuclei/Burp/Metasploit/SQLmap.
  Ship only after Phase 0's audit-log mechanism is proven in production use
  elsewhere in the product. The authorization gate specifically ships as
  the out-of-band, non-self-attested, cryptographically-verifiable design
  from the start (not a simpler in-chat version hardened later — the
  documented nation-state attack pattern this defends against exploited
  exactly that shortcut), and every finding uses the three-state
  flagged/validated/blocked schema from day one
  ([04-audience-security-researchers.md](04-audience-security-researchers.md)).

## Phase 5 — Additional audiences + polish

- Technical writer, QA/test engineer, PM packs (each reusing prior-phase
  infra — see [08-audience-additional.md](08-audience-additional.md)).
  QA's primary quality gate is mutation testing, not coverage percentage,
  from the start (coverage % is documented as the wrong metric family
  entirely — 20.32% mutation scores despite green CI); PM's PRD generation
  requires citation to raw source artifacts from the start (not a
  post-hoc fact-check pass); technical-writer doc generation pulls from
  closed tickets/PR comments alongside code from the start. None of these
  are safe to ship as a simpler v1 with the guardrail added later, since
  the guardrail *is* what prevents the documented failure mode (silent
  wrong tests, generic PRDs) each pack exists to solve.
- Decide merge-vs-separate for the SRE pack based on observed demand.
- Mobbin-like design reference library — only after the legal/IP review
  noted in [05-audience-designers.md](05-audience-designers.md).
- Incremental, hash-gated knowledge graph (the full "graphify" layer) on top
  of the Phase 1 dependency graph, using a LazyGraphRAG-style
  defer-summarization-to-query-time approach
  ([10-cost-optimization.md §5](10-cost-optimization.md)) — the deepest
  remaining architectural investment; pursue once the cheaper Phase 0-2 wins
  are stable and there's a real multi-session/repeated-query workload to
  amortize the indexing cost against.
- Local-model quantization defaults (Ollama/GGUF Q4_K_M) hardened for
  general use; vLLM-class continuous-batching backend if/when a self-hosted
  multi-user server mode ships.
- **Cloud task execution** as a fourth execution target
  ([12-cloud-task-execution.md](12-cloud-task-execution.md)) — credential-
  broker proxy, portable capability-grant objects, and cloud-specific
  mandatory injection defenses, built on top of the Phase 1 worktree/
  best-of-N scheduler and the Phase 0-2 capability/audit substrate. Ship
  this *after* Phase 2's injection defenses are solid, not before — every
  documented cross-vendor cloud-agent incident (Devin's AWS-key leak, a
  2026 Claude Code Action comment-hijacking pattern, Gemini CLI's headless
  RCE) is a lethal-trifecta variant, and cloud is structurally the highest-
  risk venue for it (independent egress, often unattended for long
  stretches).

## Phase 6 — Higher-evidence extended audience packs

- **Open-source maintainers**
  ([16-audience-oss-maintainers.md](16-audience-oss-maintainers.md)) —
  the lowest-dependency pack in this phase: it's the orthogonal-review and
  worktree-isolation infra from Phase 1 plus the AI Slop Index from
  Phase 2, pointed at external contributions instead of the user's own
  agent output, plus one new contributor-trust tier. Can ship as soon as
  Phase 2 is stable.
- **Data / ML engineers**
  ([14-audience-data-ml-engineers.md](14-audience-data-ml-engineers.md)) —
  depends on the full graphify layer from Phase 5 (lineage-grounding is
  the primary value here, not a cost optimization on top of one), the two
  new `schema-write`/`backfill-execute` capability-grant types, and a
  warehouse-credit dimension on the cost estimator.
- **Legal / contract-review**
  ([13-audience-legal.md](13-audience-legal.md)) and **customer support**
  ([15-audience-support-engineers.md](15-audience-support-engineers.md)) —
  both depend on the Phase 1 grounding-gate mechanism and Phase 2's
  confidence-signal mechanism being production-proven (citation-grounding
  and policy-grounding are higher-stakes, court-tested failure modes — see
  the evidence in both files — so these should not ship against an
  unproven verification pipeline). Support engineers additionally depend on
  the blast-radius-bound mechanism from Phase 0 for the refund/credit-
  issuance capability.
- Finance/FP&A, platform-engineering, GRC, and localization extensions
  ([08-audience-additional.md](08-audience-additional.md)) ship opportunistically
  alongside the packs they extend (Analysts, DevOps, and Technical Writers
  respectively) rather than on a fixed schedule — each is a connector/
  reporting addition to an existing pack, not a new dependency chain.

## Sequencing principle

Every phase after 0 is optional and reorderable *except* that Phase 2
(security-core hardening) must land before any pack that ingests untrusted
external content (web pages, PRs, tickets) at scale, and Phase 4's
security-researcher pack must never ship ahead of Phase 0's audit log. Cost
optimizations (Phase 0-1, deepened in Phase 5) must never be sequenced ahead
of the verification/de-hallucination gate they sit upstream of — a cascade
or a cache is only as safe as the check it still has to pass. Cloud task
execution (Phase 5) must never ship ahead of Phase 2's injection defenses —
of every execution target, cloud is the one where an unattended session
with independent network egress has the most time to act on a poisoned
input before a human notices. Within Phase 6, legal and support-engineer
packs must not ship ahead of Phase 2 for the same reason the sequencing
principle already applies elsewhere: both packs exist specifically to
prevent court-documented hallucination failures (_Mata v. Avianca_,
_Moffatt v. Air Canada_), so shipping them against an unproven
verification pipeline would be shipping the exact failure mode they're
supposed to close.
