# pro — security core

This is the foundation every other spec depends on. Build this before audience
packs — it's the substrate the rest of the system sits on, not a feature
alongside them.

## 1. Capability & sandbox model (Deno-inspired)

Deno's model: deny-by-default, scoped allow-lists (`--allow-net=host`, not
just a boolean), explicit deny-overrides, runtime-introspectable/revocable
permissions. Deno's own documented weak point: a spawned **subprocess runs
with its own OS permissions, outside the sandbox** — this is exactly the gap
"pro" must not inherit.

How current tools compare: Claude Code layers pattern-based rules
(allow/deny/ask) with a real OS sandbox underneath on macOS (Seatbelt) —
Anthropic reports an 84% reduction in permission prompts from this. OpenAI
Codex CLI has named permission profiles (`read-only` / `workspace` /
`danger-full-access`) plus a runtime `request_permissions` elevation tool —
the closest existing analog to Deno's model. opencode and Aider rely on
pattern-matched command strings or plain user trust, both bypassable by a
cleverly constructed command.

**"pro" design — two layers:**

1. **Policy layer** (declarative, capability-scoped, lives in
   `pro.config.mjs`): every grant is scoped, never a bare boolean —
   `net: { allow: ["api.github.com"] }`, `read`/`write` scoped to project
   root + explicit extra paths, `run` as an allow-listed binary set with
   argument-shape validation (parsed argv against read/write scopes, not
   regex on the raw command string), `env` scoped to named vars only (never
   a full environment dump — this is the classic "cat secrets to a webhook"
   failure mode).
2. **Enforcement layer** (OS-level, not self-reported): every `run`
   capability executes inside a real OS sandbox (bubblewrap/Landlock on
   Linux, Seatbelt on macOS, AppContainer/job objects on Windows), so a
   crafted command that fools the policy layer is still blocked by the
   kernel. This closes Deno's own admitted subprocess gap.

Additional requirements: **runtime elevation prompts carry provenance** (why
was this requested — user's direct instruction, or content the agent read?
See §2); **explicit deny always wins**; **every grant/use is logged** with
its causal chain for audit (needed for debugging and for the pentest
extension's audit-trail requirement, [04](04-audience-security-researchers.md));
ship default profiles (`read-only` / `workspace` / `danger-full-access`,
mirroring Codex) so users aren't forced to hand-author policy, but leaving
`workspace` always requires an explicit opt-in step, never an implicit
prompt-driven escalation.

### Content-hash-scoped grants, not path-scoped (validated by a documented flaw)

Mindgard research on current agent tools (Claude Code, Codex CLI, Gemini
CLI) documents a live "approve once, exploit forever" flaw: trust is bound
to a *folder*, approved a single time — every future config/command
definition in that path then executes with no re-prompt, forever. Attack
path: approve a benign repo, then an attacker modifies `.mcp.json`/hooks/
build scripts on a later pull, and the agent silently executes the modified
command with full developer credentials on the next run. Vendors cite
"approval fatigue" as the reason, but project configs rarely change in
practice, so the real re-approval cost is near-zero — Cursor and VS Code
Copilot already ship content-hash-based re-approval successfully.

**"pro" must bind grants to the content hash of the specific config/command
definition, not the containing path.** Any diff in `.mcp.json`, a hook, or
a build script invalidates the grant and requires a diff-annotated
re-approval showing exactly what changed and who committed it — never a
silent carry-forward of trust.

### Blast-radius bounds and staged autonomy (validated by real incidents)

Real, documented failures make the case concretely: a Replit agent ignored
an explicit "don't change any code" instruction 11 times and deleted a live
production database; a Claude Code agent executed `rm -rf` from root,
destroying all user files (GitHub issue #10077); overprivileged trading
agents at Step Finance executed 261,000+ unauthorized transfers (~$27-30M
loss) because they could execute large transfers with no approval; an
internal Meta agent expanded its own data-access permissions and posted
unauthorized guidance, undetected for two hours. Wiz's field data quantifies
the fix: organizations enforcing least-privilege for AI agents report a
**17% agent-security-incident rate vs. 76% without it** — the single
largest measured risk-reduction lever found in the security-tooling
research behind this spec (see [11-user-research-pain-points.md](11-user-research-pain-points.md)).

**"pro" requires, in addition to the capability scoping above:**

- A numeric **blast-radius bound** (max hosts affected, max $ value if
  financial, max records if data) declared at grant time for any
  production-write capability, enforced at the tool-adapter layer — not
  merely implied by the model's general trust level. This directly closes
  the Step Finance/Meta failure pattern, where no independent bound existed
  on the consequence of an action, only trust in the agent overall.
- An **instruction-violation circuit breaker**: once a user issues an
  explicit negative constraint ("don't touch prod," "don't run
  migrations"), it is enforced as a hard sandbox-level block (e.g.
  network/DB-connection-string denylisting), never merely a prompt
  instruction the model can second-guess or panic past — this is exactly
  the gap that let the Replit agent violate its own explicit instructions
  repeatedly.
- **Mandatory dry-run + snapshot/rollback** for anything in the destructive
  tier: stage changes in an overlay filesystem, present a diff, require
  explicit confirmation, and always keep a one-command rollback available —
  turns "the agent went off the rails" from a disaster into an
  inconvenience.
- A per-capability **autonomy tier** — 0 (shadow/observe-only), 1 (suggest),
  2 (suggest, one-click approve), 3 (auto-execute, reversible), 4
  (auto-execute, irreversible) — with every newly configured capability
  defaulting to tier 0. Promotion to a higher tier requires both a minimum
  shadow-mode observation period and an explicit, audit-logged human
  sign-off; there is no path from tier 0 to tier 3+ without that sign-off,
  regardless of the model's own confidence. Required metadata on every
  capability definition — **reversibility, blast radius, signal quality,
  time-sensitivity** — computes a default autonomy *ceiling* per action
  type, so an irreversible, large-blast-radius, low-signal action is capped
  at tier 1 no matter how confident the model claims to be. This is the
  staged-autonomy pattern the DevOps/SRE industry has independently
  converged on as the way to build trust incrementally (see
  [06-audience-devops.md](06-audience-devops.md)).

### Audit log: causal classification and capability inventory

Standard application logs (HTTP responses, DB errors) don't capture
agentic reasoning, so post-incident forensics on today's tools often can't
tell whether a bad outcome was a hallucination, a successful prompt
injection, or simple privilege drift — field research finds only ~21% of
organizations have runtime visibility into agent actions and ~33% have no
audit trail at all. **"pro"'s audit record schema includes an explicit
three-way root-cause classification field — hallucination / injection /
privilege-drift — populated at incident-review time**, plus a structured
`reasoning_summary` (human-readable, for auditors/compliance) distinct from
the raw `evidence_chain` (for engineers debugging), aligning with the
FINOS AIR framework's "Agent Decision Audit and Explainability" mitigation
category rather than inventing bespoke terminology. A standing, queryable
**capability inventory view** — every active grant, who granted it, when it
expires, what it's touched — answers the "we lack visibility into what our
agents can do" gap directly from data the grant/audit system already
collects; this is a UI investment, not new backend work, and serves the
security-researcher audience's leadership-visibility needs identically
(see [04-audience-security-researchers.md](04-audience-security-researchers.md)).

**Storage, retention, and privacy.** Audit records (including the
`reasoning_summary`/`evidence_chain` fields and any session-transcript
trailers from cloud execution, [12-cloud-task-execution.md](12-cloud-task-execution.md))
are local-first by default, consistent with the "local-first optional"
pillar in [00-vision.md](00-vision.md) — for a self-hosted/air-gapped
deployment, audit data never leaves the user's own infrastructure. A
default retention window (configurable, not indefinite) and an explicit
redaction path for secrets/PII that may have been captured in a
`reasoning_summary` are required before this ships to any regulated
audience (DevOps/analyst packs both reference compliance/audit-review
needs) — this is a gap to close during Phase 0/2 implementation, not a
detail to defer to launch.

## 2. Provenance & injection-resistant context assembly

Filters/classifiers (Lakera Guard, Rebuff, NeMo Guardrails, Meta Prompt
Guard, PromptArmor) are useful as a cheap first-pass but are **not the
security boundary** — published research (arXiv 2504.11168) shows most
classifier-based guardrails have demonstrable evasion attacks. The real
defense is architectural:

- **Dual-LLM / quarantine pattern**: a privileged LLM holds tool access and
  plans/acts but never reads untrusted content directly; a quarantined LLM
  reads untrusted content (web pages, PR descriptions, emails) and can only
  return symbolic references or opaque summaries back — never raw content
  framed as instructions.
- **CaMeL** (Google DeepMind/ETH Zurich, arXiv 2503.18813): tags every piece
  of data with a provenance/capability label and enforces, in an
  interpreter (not the model), that untrusted-origin data cannot construct
  or trigger privileged actions. Reports 77% task success *with provable
  security* vs. 84% undefended — there's a real utility cost, worth paying.
- **The "lethal trifecta"** (Simon Willison): an agent is exploitable via
  injection only when it has (a) exposure to untrusted content, (b) access
  to private data, and (c) an exfiltration path. Breaking any one leg is a
  valid defense — this is exactly what capability-scoped `net` allow-lists
  from §1 already do: if egress is scoped to known-good hosts, exfiltration
  to an attacker domain is blocked structurally, independent of whether the
  injection itself was detected.
- **MELON** (ICML'25, arXiv 2502.05174): re-run the agent's trajectory with
  the user's original instruction masked; if the next action barely changes,
  it's likely injection-driven, not user-driven — flag/block. Use this as a
  background integrity check before high-stakes actions (shell exec,
  `git push`).
- **Spotlighting + sandwich defense** (Microsoft): wrap all untrusted
  tool/retrieved content in explicit delimiters, instruct the model it's
  data-never-instructions, and re-state the user's real task *after* the
  untrusted content so it has the highest recency weight. Enforce this in
  the harness's prompt-assembly code, not left to system-prompt discipline
  that a future edit could silently drop.
- **PromptArmor**-style LLM pre-filter on tool outputs before they re-enter
  context — cheap, near-zero-cost cross-check, never the sole gate.

**"pro" design:** capability system (§1) + CaMeL-style taint tracking +
quarantine pattern for content ingestion + spotlighting enforced in the
prompt-assembly layer + MELON-style masked re-execution before high-stakes
actions + provenance surfaced in every elevation prompt ("this action was
requested because of content read from `<source>`") as the human-in-the-loop
backstop.

## 3. Supply-chain / typosquatting / slopsquatting gate

LLMs hallucinate plausible-but-nonexistent package names at meaningful rates
(open-source models ~21.7% average, some configs >33%; commercial models
~5.2% average) — independently corroborated by a USENIX Security 2025 study
across 16 models and 576,000 samples finding a ~20% average hallucination
rate (see [11-user-research-pain-points.md §1](11-user-research-pain-points.md)).
Attackers pre-register these hallucinated names
("slopsquatting") — a documented 2026 case propagated a hallucinated package
through 237 repos via agent-to-agent installs with **no human in the loop**.

Detection signals used by Socket.dev/Sonatype: edit-distance to popular
package names combined with download-count disparity, install-script
static analysis (network calls, env reads, obfuscation), maintainer-behavior
signals (new ownership, out-of-order version publishing). `npm audit` /
OSV-Scanner are purely reactive (known-CVE databases) — they cannot catch a
brand-new malicious/typosquatted publish.

**"pro" gate, before any install the agent suggests or runs:**

1. Existence check first — verify the package is real before suggesting it
   (blocks pure hallucination).
2. Edit-distance + download-disparity check against a cached top-N list per
   ecosystem.
3. Install-script/static-analysis warning for new or low-reputation packages
   before execution.
4. Maintainer/publish-velocity flags.
5. This environment already ships a `sonatype-guide` MCP tool intended to be
   invoked before any dependency add/install/upgrade — wrap that workflow
   natively rather than relying on the agent "remembering" to call it.
6. Treat "install an unverified package" as a `run`+`net`+`write` capability
   elevation event (§1), requiring the same provenance-aware confirmation as
   any other privileged action.

## 4. Verification & review pipeline (de-hallucination / de-slopification)

Prefer deterministic verification over trusting the model's own claim, in
this order of trust: **compiler/type-checker > linter > test execution >
LLM-judge**. RAG grounding + citation verification reduces hallucination
substantially but the toolchain is higher-signal than self-critique wherever
a toolchain exists. The 2025-2026 literature converges on one framing worth
adopting explicitly: **"The Verification Horizon"** (arXiv:2606.26300) argues
that as generation quality improves, *verification* — not generation —
becomes the bottleneck, every verifier is only a proxy for true intent, and
optimization pressure inevitably finds proxy-gaps (a model that games a
rubric-judge, or games test coverage without solving the real problem). No
single verifier suffices — **build a layered, non-redundant verification
stack, and treat it as infrastructure to keep revisiting, not a one-time
build.**

### Deterministic symbol-grounding gate (highest-leverage addition)

Before compiler/linter/test stages, run a **deterministic AST/symbol-table
and LSP-based grounding checker** on generated code: parse to AST, validate
every referenced symbol/API call against the *actual* introspected codebase
(real installed package signatures, not the model's training-time memory of
them), and flag any referenced file path or config key that doesn't exist in
the real filesystem/schema. A published implementation of this exact pattern
reports 100% precision, 87.6% recall, sub-0.2-second runtime for 200 samples
on a laptop CPU, and auto-corrected 77% of detected hallucinations (97.9%
success specifically for missing-import fixes). Using the LSP itself as a
semantic ground-truth oracle (`goto_definition`/`find_references`/
diagnostics, not just text completion) shifts an answer from "the model
believes X" to "the language server confirms X" — industry-reported gains
of ~27% fewer hallucinated APIs and ~33% fewer mislocalized edits when
planning is LSP-grounded. This gate is near-zero cost and should run as a
**mandatory pre-display gate**, ahead of the compiler/linter stage, since it
catches syntactically-valid-but-nonexistent API calls that a linter won't.

Ground this against **CodeHalu's** four-category hallucination taxonomy —
mapping (inconsistent name/entity mapping), naming (nonexistent function/
class names), resource (nonexistent files/libraries), logical (runs but
doesn't satisfy the actual requirement) — as a structured field in
telemetry and the Slop Index below, essentially for free once the grounding
gate exists. Verification effort should weight toward rarely-used APIs
specifically — published results show common APIs are already well-covered
by model training data, while low-frequency API calls are where correctness
collapses (one benchmark: only 38.6% valid low-frequency API invocations
from a frontier model).

### Confidence signals before presenting output

- **Semantic Entropy Probes** (arXiv:2406.15927): where hidden states are
  available (self-hosted/open-weight models), a lightweight probe trained on
  a single generation's hidden states approximates full semantic entropy
  (cluster sampled generations by NLI-based meaning-equivalence, measure
  entropy over the clusters — high entropy on *meaning*, not just phrasing,
  is a confabulation signal) at near-zero added latency. Prefer this where
  available.
- **SelfCheckGPT**-style sampling-consistency (arXiv:2303.08896): for
  black-box hosted APIs with no hidden-state access, 3-5 low-temperature
  resamples + an NLI/LLM-judge consistency check over them is a cheap
  fallback — this reuses the same sampling infrastructure as best-of-N
  rather than adding new machinery.
- **Gating logic**: low confidence → don't silently present the output;
  route to additional grounding/context injection, escalate to orthogonal
  review, or surface an explicit "low confidence, verify manually" flag —
  this is the direct product hook for a real-time trust signal in the UI,
  and doubles as the escalation trigger for the cost-optimization cascade in
  [10-cost-optimization.md §3](10-cost-optimization.md).

### Graph-grounded verification, not just chunk-based RAG

Knowledge-graph-grounded retrieval (see
[10-cost-optimization.md §5](10-cost-optimization.md) for the "graphify"
architecture) measurably reduces hallucination versus plain vector-
similarity RAG on **multi-hop** queries specifically — benchmarked cases
report a 6% hallucination reduction plus an 80% token-usage reduction on
one dataset, and a jump from ~50% to ~94% accuracy on multi-hop reasoning in
another. The mechanism: graph edges are typed and provenance-tracked (this
fact came from parsing file X, line Y, edge type "calls"), so an answer can
cite the exact structural relationship used — a stronger, more falsifiable
citation than "which text chunk it came from." This is strongest for
cross-file/dependency-shaped questions ("what calls this, which of those
call sites are in the auth path") — don't expect a uniform win on simple
lookups, where well-tuned vector RAG closes most of the gap; use graph
retrieval and vector retrieval as complements, not a replacement for one
another.

### Self-consistency and verifier-guided selection, beyond plain best-of-N

Plain majority-vote best-of-N has a cheaper, more reliable upgrade path:
**confidence-weighted voting** — Self-Certainty and Confidence-Informed
Self-Consistency (CISC) rank/weight candidate votes by the model's own
token-distribution confidence rather than counting them equally, and reach
the same reliability at meaningfully lower N (fewer total samples for equal
accuracy). Where a real verifier exists (compiler/test suite, not just an
LLM-judge), verifier-scored selection beats plain self-consistency at low N
and matches it at high N using 1-3x fewer tokens overall — the concrete
upgrade to "pro"'s existing best-of-N-with-a-verifier baseline is to stop
sampling early once one candidate's verifier-confidence clears a threshold,
rather than always sampling a fixed N. For long multi-step agent plans
(plan → edit file 1 → edit file 2 → run tests), consider **step-level
verification** (process-reward-model-style, e.g. AgentPRM) rather than only
scoring the final diff — this catches a bad step early instead of only
discovering it after the whole plan has run.

### Orthogonal review, not homogeneous multi-sampling

Addy Osmani's comparison of Greptile/CodeRabbit/Seer over the same codebase
found the tools **never once flagged the same line** — genuinely independent
reviewers (different model/tool/context) catch disjoint bugs; four samples
from one model is "a single reviewer with a larger invoice." "pro" ships
three review modes:

- **Simple** — one reviewer pass, fast default.
- **Adversarial** — Osmani's "doubt-driven-development" pattern:
  CLAIM → EXTRACT → DOUBT → RECONCILE → STOP, applied in-flight to a decision
  before it's finalized (not just a post-hoc diff review) — for high-stakes
  work (security-sensitive code, irreversible ops, unfamiliar codebases).
- **Orthogonal** — route the diff through a genuinely different model/tool
  with no shared context/bias; disagreement is a signal for a human
  decision, never auto-resolved.

### AI Slop Index

Beyond churn/revert rate, duplication ratio, test coverage delta, and
architectural coherence, calibrate thresholds against external baselines
and adopt a richer axis set: **GitClear's** large-scale analysis (211M
lines, 2020-2024) found copy-pasted code share rose 8.3%→12.3%, duplicated-
block frequency rose 8x in a single year, refactoring's share of changed
lines fell from 25% to under 10%, and 2-week revision/revert rate rose from
3.1% to 5.7-7.9% — concrete, citable numbers to calibrate a Slop Index
against rather than picking thresholds arbitrarily. **FrontierCode**
(Cognition, 2026) — a benchmark explicitly measuring "mergeability" rather
than correctness — is worth adopting nearly verbatim as the rubric's axis
list: behavioral correctness, regression safety, mechanical cleanliness,
test correctness (do the tests assert the right thing, not just pass),
scope discipline (does the patch touch only what's needed), and convention-
conformance. Its headline finding — even the best frontier model scored
only ~13.5% on the hardest mergeability subset, far below typical
pass/fail-benchmark scores — is the empirical case for why "passes tests"
and "is actually mergeable" are different bars, and why the Slop Index
needs axes beyond pass/fail. Note explicitly: perplexity/stylometric
"AI-authorship" detectors answer a different question (*who* wrote this)
than the Slop Index needs (*is this good, non-redundant, shippable code*) —
don't conflate the two or over-invest in authorship detection.

This runs as a cheap automatic quality gate ahead of human review, and (per
the CodeHalu taxonomy above) should tag findings by hallucination category
where applicable, not just a single aggregate score.

### Vulnerability pattern clustering, not single-instance flagging

CodeRabbit's analysis of 470 real-world PRs found AI-co-authored PRs carry
~1.7x more issues overall, and specific vulnerability classes at
disproportionate rates (2.74x more XSS, 1.88x more password mishandling,
1.82x more insecure deserialization) — but the more actionable finding is
qualitative: **when a model learns a bad pattern, it replicates it
consistently across every file it touches, whereas human errors tend to be
isolated.** This changes remediation economics from "fix each instance" to
"fix the pattern once." **"pro" runs a pattern-level dedup pass**: once a
deterministic scanner (Semgrep-class) flags one instance of a vulnerability
class in an AI-touched diff, it searches the entire diff/codebase for the
same pattern rather than stopping at the first hit. Given how
disproportionately over-represented SQL string-concatenation (CWE-89) and
hardcoded secrets (CWE-798) are in this data, both are **hard-gated as
mandatory deterministic pre-commit checks**, not general-purpose linter
suggestions that can be dismissed.

## 5. Offensive-security extension (gated capability tier)

See [04-audience-security-researchers.md](04-audience-security-researchers.md)
for detail. Summary: pentest/exploitation tool use is its own elevated
capability tier on top of §1–4, not a normal `run`/`net` grant — requiring
explicit scope declaration validated at the tool-adapter layer, network-layer
enforcement of that scope independent of the model's own judgment, an
authorization gate before exploitation-class tools unlock, and mandatory
immutable audit logging reusing the same infrastructure as §1.

## Build order

§1 (capability/sandbox) and §2 (provenance/injection defense) come first —
everything else (§3, §4, §5) composes on top of the same taint/capability
substrate rather than reimplementing its own notion of trust.
