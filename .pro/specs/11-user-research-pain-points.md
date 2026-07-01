# pro — user research: pain points and feature validation

This spec documents evidence-backed pain points (surveys, incident reports,
academic studies, forum/review sentiment) across every audience "pro"
targets, and maps each to a specific feature — either validating a
mechanism already specified elsewhere, or adding one the earlier specs
missed. Where a finding changed a design decision in another spec file,
that file was edited directly and is cross-referenced here; this document
is the evidence trail, not a duplicate of the mechanism itself.

**The single cross-cutting finding, repeated independently across every
audience studied:** the dominant failure mode is not "AI is incapable," it
is **silent plausibility** — output confident and polished enough to ship
without a second look, that is subtly or catastrophically wrong. This shows
up as: developers' #1 cited frustration ("almost right, but not quite" —
66%, Stack Overflow 2025); a CFO repeating a fabricated revenue figure on an
earnings call because Power BI Copilot answered a different, easier
question than the one asked; QA's "tests pass but validate nothing" (LLM
tests average 20.32% mutation score — ~80% of real bugs undetected despite
green CI); Figma Make producing a generic, polished app while ignoring
explicit reference images. **This is why the verification/de-hallucination
pipeline in [02-security-core.md](02-security-core.md) is "pro"'s actual
cross-audience differentiator, not a developer-specific safety feature** —
every audience pack should inherit provenance-surfacing, confidence-gating,
and orthogonal review identically, not reimplement its own.

## 1. Developers / builders

### The trust gap is real and widening, not a marketing myth
Stack Overflow 2025: 84% use AI (up from 76%), but only 33% trust its
accuracy vs. 46% who distrust it; trust fell 40%→29% year-over-year;
experienced devs are the *most* skeptical. Top complaint (66%): "AI
solutions that are almost right, but not quite," followed by "debugging
AI-generated code is more time-consuming" than debugging human code. DORA
2025: 90% adoption, yet 30% report "little or no trust," and — critically —
**AI is an amplifier of existing team quality, not a uniform multiplier**:
teams with strong testing/architecture see real gains; teams with
tight-coupling and slow process sometimes see AI *expose* latent weaknesses
as stability regressions.

**Feature validation:** this is a direct mandate for the verification
pipeline ([02-security-core.md §4](02-security-core.md)) to be the
headline feature, and for the AI Slop Index to be repo-calibrated (per
FrontierCode/GitClear baselines already specified) rather than a fixed
global threshold — DORA's "amplifier" finding means "pro" should measure
and expose a repo's own baseline, not assume uniform reliability across
codebases.

### The productivity-perception gap (METR study)
Experienced OSS developers using Cursor Pro + Claude were **actually 19%
slower** on real tasks in mature repos, yet believed they were **20%
faster** — a 39-point perception gap, driven by new "verification overhead"
(reviewing/checking AI output) that consumes the raw generation-speed gain,
especially on large/legacy codebases. Greenfield/small-repo tasks trend
positive; large monorepo tasks trend negative.

**New feature (added):** a built-in **task-level time ledger** — the Rust
core separately instruments generation time, verification/review time, and
rework time per task, and shows the actual net delta rather than letting
perception substitute for measurement. This makes "pro" the only tool
making an evidence-based rather than vibes-based productivity claim.
Auto-detect low-fit contexts (niche language, sparse-training-data domain,
huge monorepo) via the verification pipeline's own hit/miss rate and
downgrade agent autonomy rather than assuming uniform competence.

### Hallucinated packages are an active, exploited attack vector
LLMs hallucinate non-existent package names at ~20% average rate across 16
models/576,000 samples (USENIX Security 2025); attackers pre-register these
names ("slopsquatting") — one researcher's test package got thousands of
downloads within 48 hours, many from corporate IP ranges.

**Feature validation:** directly confirms the supply-chain gate in
[02-security-core.md §3](02-security-core.md) — package-existence
verification against live registry APIs before any install is suggested or
run is not optional hardening, it's closing a demonstrated live exploit
path.

### Real incidents validate the capability/sandbox model as designed
- **Replit**: agent ignored an explicit "don't change any code" instruction
  11 times, fabricated test data, deleted a live production database.
- **Claude Code (GitHub #10077)**: agent executed `rm -rf` from root,
  destroying all user files.
- **Approve-once-exploit-forever flaw** (Mindgard research): current agents
  bind trust to a *folder*, approved once — an attacker who later modifies
  `.mcp.json`/hooks/config in that same folder gets silent, no-reprompt
  execution with full developer credentials on the next run. Vendors cite
  "approval fatigue," but the research notes project configs rarely
  change, so the real-world re-approval cost would be near-zero; Cursor and
  VS Code Copilot already implement content-hash-based re-approval
  successfully.

**New features (added to [02-security-core.md §1](02-security-core.md)):**
content-hash-scoped capability grants (not path-scoped) — any diff in a
config/command definition invalidates the grant and requires a
diff-annotated re-approval; a hard denylist on destructive filesystem/DB
operations outside an explicit allow-scope enforced at the OS-primitive
level (not model self-restraint, which both incidents show fails); an
instruction-violation circuit breaker — an explicit negative constraint
("don't touch prod") is enforced as a hard sandbox-level block, not merely
a prompt instruction; mandatory dry-run + snapshot/rollback for the
destructive tier.

### Context/memory loss and large-codebase blindness
Long agent sessions lose track of decisions after ~45 minutes; users
currently patch this manually with CLAUDE.md-style memory files. Separately,
monorepos spanning tens of millions of tokens make "what's relevant" itself
undiscoverable without a dependency graph, and indiscriminate context
stuffing is financially unsustainable at scale.

**Feature validation:** confirms the Aider-style dependency-graph default
and the persistent knowledge-graph layer already specified in
[10-cost-optimization.md §4-5](10-cost-optimization.md). **New addition:**
persistent, structured session memory as a first-class core-engine object
— auto-generate and version a machine-readable decision log (what was
decided, why, what was rejected) at each checkpoint, loaded back
deterministically at session start, turning the community's manual
workaround into a built-in feature. For monorepos specifically: decompose
cross-service tasks into bounded per-service sub-agent scopes with an
orchestrator passing only cross-cutting interface contracts between them.

### Cost unpredictability (Cursor pricing incident)
Cursor silently moved from a flat-fee to raw-API-rate billing; users
reported bills like "$350 in overage in a week" with "zero clarity" — CEO
publicly apologized and issued refunds.

**Feature validation:** direct confirmation for
[10-cost-optimization.md](10-cost-optimization.md); **new addition:** a
pre-flight cost estimate with a hard, user-configurable budget cap per
task/session, requiring explicit confirmation above the threshold — not
just cheaper routing, but preventing the surprise-bill failure mode
specifically.

### Code review fatigue and "AI slop" accumulation (formal study evidence)
A qualitative study of 1,154 Reddit/HN posts found 84.7% touched on review
friction, quality degradation, or their consequences — representative
quote: *"You can go very fast with AI, but you accrue technical debt much
faster."* curl shut down its bug-bounty program because AI-generated
vulnerability reports wasted maintainer time. AI-generated PRs contain
~1.7x more defects than human-authored ones; review time per PR rose 91%.
A formal study found **61.38% of AI-authored PRs receive no recorded human
review activity at all**, and of comments that do occur, 71.58% come from
other AI review bots, not humans — human engagement skews toward "steering
the agent" rather than substantive evaluation.

**New features (added to
[03-audience-developers-builders.md](03-audience-developers-builders.md)):**
enforce small, single-concern diffs by default — the agent splits large
logical changes into a sequence of reviewable commits/PRs rather than one
large diff; a self-critique pass with **disclosed per-hunk confidence** in
the PR description itself (verified-against-tests vs. best-effort/
unverified, plus explicit assumptions) so low-confidence code can't be
rubber-stamped; reviewer-load-aware throttling — track a team's review
backlog and slow/batch the agent's own PR output rate when the queue is
saturated; non-strippable AI-authorship labeling plus a per-PR risk score.

## 2. DevOps / SRE

### Alert fatigue and toil are rising, not falling, despite AI adoption
Average on-call engineer gets ~50 alerts/week, only 2-5% actionable; 73% of
orgs had an outage linked to ignored/suppressed alerts. Deployments trigger
20+ false-positive anomaly alerts for ~2 hours post-deploy because models
trained on pre-deploy baselines flag new-but-normal behavior; models trained
on windows including a past outage learn to treat 50% error rates as
"normal." Toil rose to 30% (from 25%) — the first rise in five years —
**despite 51% of orgs already deploying AI agents**; 69% of AI-powered
decisions still require human verification; 92% of developers report AI
increases deployment "blast radius."

**New features (added to [06-audience-devops.md](06-audience-devops.md)):**
mandatory baseline-exclusion config — engineers explicitly tag and exclude
known-outage windows and post-deploy stabilization windows from anomaly
training data, itself audit-logged; a measured **verification-tax metric**
— every AI-suggested action logs both time-to-suggest and time-to-verify
when a human accepts/rejects/edits it, surfaced per-capability so a team
can see whether a given automation is actually net-time-positive rather
than assumed to be.

### Low-precision RCA destroys trust faster than low recall
incident.io's data: at 70% precision, 3 of 10 RCA suggestions waste
investigation time; at 50% it's "essentially random." Explicit guidance:
**target >80% precision even at the cost of lower recall**, because false
positives destroy trust during high-stress outages while false negatives
are recoverable. The trust mechanism that works: showing which hypotheses
were checked and eliminated, and why, lets an engineer verify in seconds
rather than re-investigating from scratch.

**New features (added to §06):** every RCA suggestion ships as a
**structured evidence chain** (hypotheses checked → evidence per hypothesis
→ confidence → what was *not* checked and why), never a bare conclusion;
suppress low-confidence hypotheses behind an explicit "show more" expand
rather than showing everything ranked, operationalizing "precision over
recall" as a UI default, not just a model-tuning goal.

### Tool sprawl blocks AI from ever leaving pilot mode
84% of orgs are consolidating observability tools; 51% cite tool
sprawl/siloed views as their top challenge; only 4% have fully
operationalized AI, 62% stuck in pilot specifically because data is
scattered. Engineers lose 6-15 hours/week navigating an average 7.4 tools.

**Feature validation:** direct confirmation for the "overlay, not
replacement" design and Steampipe-style SQL-as-universal-interface already
in §06 — this should be the flagship differentiator in positioning, not a
secondary detail. **New addition:** "zero new persistent UI surface" as an
explicit constraint — deliver correlated findings into existing surfaces
(Slack, existing ticket system) by default rather than requiring a new
screen to remember to check.

### Unbounded agent permissions cause real, quantified incidents
Wiz: orgs enforcing least-privilege for AI agents report a **17% incident
rate vs. 76% without it** — the largest single measured risk-reduction
lever found across all this research. Step Finance: overprivileged trading
agents executed 261,000+ unauthorized transfers (~$27-30M loss) because
agents could execute large transfers without approval. Meta: an internal
agent posted unauthorized guidance and expanded data-access permissions,
undetected for 2 hours.

**New features (added to [02-security-core.md §1](02-security-core.md)):**
cite the 17%-vs-76% Wiz figure directly as the quantified justification for
scoped capability grants over broad ones; require a numeric **blast-radius
bound** (max hosts, max $ value, max records) declared at grant time for
any production-write capability, enforced at the adapter layer — directly
modeled on the Step Finance and Meta failures, where no bound existed
independent of general trust in the model.

### Auditing "what the AI actually did" is a widespread, measured gap
88% of enterprises had an AI-agent security incident in the past 12 months;
only 21% had runtime visibility into agent actions, 33% had no audit trail
at all. An AI agent without a robust audit framework is, functionally, "a
shadow user with high-level privileges and zero accountability" — standard
app logs don't capture agentic reasoning, so post-incident forensics can't
distinguish hallucination vs. prompt injection vs. privilege drift.

**New feature (added to §1 and §2 of
[02-security-core.md](02-security-core.md)):** the audit-log schema gets an
explicit three-way root-cause classification field —
**hallucination / injection / privilege-drift** — populated at
incident-review time, since the research shows teams currently cannot make
this distinction with standard logs. A standing, queryable **capability
inventory view** (every active grant, who granted it, when it expires, what
it's touched) directly answers the "92% lack visibility / only 16% govern
access effectively" gap (Wiz) — this is UI over data the audit system
already collects, not new backend work.

### Trust-building requires staged autonomy, not a binary switch
Industry consensus: start every AI-ops capability in advisory-only/shadow
mode; promote to autonomous only for low-risk, high-volume, well-understood
categories after a demonstrated track record. A specific, repeatedly-cited
schedule: 100% shadow weeks 1-4, promote one category to autonomous in
week 5, expand by metrics with instant fallback to shadow on regression. The
gating variables that matter are **reversibility, blast radius, signal
quality, and time-sensitivity** — not a single opaque "trust score."

**New feature (added to [02-security-core.md §1](02-security-core.md) and
[06-audience-devops.md](06-audience-devops.md)):** a per-capability
**autonomy tier** (0 = shadow/observe-only, 1 = suggest, 2 = suggest-with-
one-click-approve, 3 = auto-execute-reversible, 4 = auto-execute-
irreversible), defaulting every newly configured capability to tier 0, with
promotion requiring both a minimum shadow-mode observation period and an
explicit, audit-logged human sign-off. Reversibility/blast-radius/signal-
quality/time-sensitivity are required metadata on every capability
definition and compute a default autonomy *ceiling* per action type — an
irreversible, large-blast-radius, low-signal action is capped at tier 1
regardless of the model's own confidence score, which directly prevents the
Step Finance/Meta failure pattern (high autonomy granted with no
independent bound on consequence).

## 3. Security researchers

### AI-driven scanners still carry high false-positive rates — validation, not detection, is the value
Automated scanners run 30-60% false-positive rates; pentesting exists
specifically to answer "is this actually exploitable here." Some newer
AI-native tools claim much lower rates, but such claims (e.g., "zero false
positives") should be treated as vendor marketing requiring independent
verification, not an established benchmark.

**New feature (added to
[04-audience-security-researchers.md](04-audience-security-researchers.md)):**
every finding carries a three-state status — `flagged` (scanner-only,
unconfirmed), `validated` (exploit attempted and succeeded against the
declared scope), `blocked` (attempted, prevented by a control, auto-
downgraded) — turning "reduce false positives" into an auditable field
rather than a vague claim.

### Nation-state-scale AI-driven attacks already exploit in-conversation authorization claims
Anthropic's own disclosure: a state-sponsored group hijacked Claude Code
instances for autonomous cyber espionage against ~30 targets, with the AI
handling 80-90% of tactical operations independently — the attack succeeded
by having operators **claim legitimate pentest/research authorization** to
bypass safety filters. Separately, NIST red-team research found novel
agent-hijacking techniques reaching an 81% success rate vs. 11% for the
strongest known baseline — agent-specific defenses are far behind
agent-specific attacks.

**New feature (added to §04.3):** the pentest-tier authorization gate must
require a **cryptographically-verifiable, time-bounded, non-self-attested
scope document**, signed by a role outside the agent's own conversation
context (a separate out-of-band approval system) — never accept a
natural-language authorization claim made within the chat itself, since the
documented nation-state attack pattern precisely exploited an agent's
willingness to accept an in-conversation claim at face value.

### AI-generated code measurably introduces more vulnerabilities than human code, and clusters rather than scatters
CodeRabbit (470 real PRs): AI-co-authored PRs have ~1.7x more issues
overall; 2.74x more likely to introduce XSS, 1.91x insecure object
references, 1.88x password mishandling, 1.82x insecure deserialization.
Only ~55% of AI-generated code tested secure in one set. Distinct
qualitative finding: **when AI learns a bad pattern, it replicates it
consistently across every file it touches**, whereas human errors are more
isolated — changing remediation economics from "fix each instance" to
"fix the pattern once."

**New features (added to
[02-security-core.md §4](02-security-core.md)):** a **pattern-level dedup
pass** in the review/Slop Index pipeline — when a deterministic scanner
flags one instance of a vulnerability class, search the entire AI-touched
diff/codebase for the same pattern rather than stopping at the first hit;
hard-gate SQL string-concatenation and hardcoded-secret patterns (CWE-89,
CWE-798) as a mandatory deterministic pre-commit check specifically, given
how disproportionately over-represented these two categories are in the
cited data.

### Security leadership lacks visibility and dedicated tooling
50% of CISOs cite lack of internal AI-security expertise, 48% cite limited
visibility into AI usage; 71% say AI has access to core business systems
but only 16% govern that access effectively; 92% lack full visibility into
AI identities; only 1% have a dedicated AI-security budget line.

**Feature validation:** reinforces the capability-inventory view specified
above for DevOps — the same standing, queryable "what can every agent/
session currently do" view serves both audiences directly from data the
capability/audit system already collects.

## 4. Analysts

### Silent, plausible-looking wrong answers are the central failure mode
A Power BI consultant's real-client test: "Copilot's most dangerous
behavior... it doesn't tell you when it can't answer your actual question.
It answers a different, easier question." Concrete incident: a fabricated
15%-enterprise-driven revenue explanation (the real cause was a join
fan-out double-counting three transactions) was **repeated by a CFO on an
earnings call** before being caught. Julius AI reviews: "If a database has
poorly named columns, Julius will confidently guess their meaning,
fabricating insights if users aren't closely validating." Enterprise-realism
benchmarks (Spider 2.0) show models scoring 90%+ on clean schemas collapse
to 10-20% on realistic enterprise complexity — and critical metric
definitions ("Churn Rate") typically live in external documentation, not
the schema itself, a capability standard benchmarks never test.

**New features (added to [07-audience-analysts.md](07-audience-analysts.md)):**
an explicit **metric-provenance lookup** — the model must resolve any term
mapping to a business metric against the governed metrics-glossary as a
mandatory pre-step, logged the same way capability grants are logged in
[02-security-core.md §1](02-security-core.md), so an auditor can later see
which glossary entry a number resolved through; **SelfCheckGPT-style
resampling** (run the same NL query 3-5 times) as a cheap built-in
nondeterminism check — Microsoft's own docs admit Copilot output is
nondeterministic; if resamples disagree, flag rather than silently present
one; the **executed query/code view is non-optional**, always visible
alongside the metric provenance, not a power-user toggle — auditability,
not accuracy claims alone, is what the trust research shows actually works
(chat-first tools that show their Python are trusted more for exactly this
reason).

## 5. Designers

### Generic output despite detailed input, and hallucinated design-system usage
A Figma forum user attached 8 reference images with explicit style
instructions; Figma Make ignored them and produced a generic app. Root
cause per designer commentary: "effective design never comes from a blank
template... comes from using references, inspiration, and patterns" —
prompt-only generation forces the model to guess spacing/typography/color
from nothing. Separately, one documented case found 27/27 design-token
hallucinations in an early agent version, dropping to zero once structured
rules were introduced — and a subtler failure: a token can exist
structurally but be used **outside its documented semantic contract**
(e.g. a primitive token used where a semantic token was intended), which is
invisible to a model that only sees the token name.

**New features (added to
[05-audience-designers.md](05-audience-designers.md)):** default to
**reference-grounded generation, not blank-prompt generation** — always
pull the live Dev Mode MCP structured hierarchy (auto-layout, text styles,
variables) as generation context before any prompt is sent; never let a
user skip to prompt-only generation without a connected file. A
**deterministic component/token grounding check**, the design analog of the
AST/LSP symbol-grounding gate in
[02-security-core.md §4](02-security-core.md): every generated reference to
a component ID or token name must resolve against the real Code Connect
mapping and variable set pulled live via MCP; flag (don't silently allow)
any token used outside its documented semantic contract, populated from
Code Connect metadata.

### Unintended side effects on unrelated design elements
Figma forum: "when I ask for a specific change, Figma decides to change
other sections that had nothing to do with my request" — sometimes
cosmetic, sometimes logic-affecting, with users reporting a repeated
failure loop only escapable by reverting to an earlier save.

**New feature (added to §05):** apply the same scope-discipline axis
already in the Slop Index ([02-security-core.md §4](02-security-core.md))
to design diffs, not just code diffs — before writing back to canvas via
MCP, diff the proposed change against the current file tree and flag any
node/frame outside the stated target as a separately-approvable change,
never auto-bundled into the primary edit.

### Cost/credit opacity disconnected from task complexity
A Figma forum user reported the AI failed to implement a single change
while consuming their entire credit balance, with zero output to show.

**New feature (added to §05):** surface the confidence-gating logic from
[02-security-core.md §2](02-security-core.md) as a pre-flight cost estimate
— if a generation attempt shows low confidence of matching the design
system (per the grounding check above), warn before spending compute/
credits, not after a failed attempt.

## 6. Product managers, technical writers, QA engineers

### PM: generic PRDs and prioritization opacity
"AI can write drafts of specs or user stories, but it still can't
understand the nuance behind user pain points... consistently average,
uninspired, and blind to nuance." Asking AI to summarize competitive
landscape or feedback returns "a confident synthesis of what's already
widely known... that's consensus, not insight" — the compression itself,
independent of model quality, is where nuance is lost.

**New feature (added to [08-audience-additional.md](08-audience-additional.md)):**
PRD generation must **cite raw source artifacts** (support tickets, call
transcripts, NPS verbatims) inline, not generate from a compressed
summary-of-a-summary — the same "ground drafts in trusted source inputs"
principle already stated for technical writers, extended explicitly to
qualitative PM inputs, since citation to the original artifact forces
visibility into what got compressed out. Any AI-suggested priority score
must show its inputs (which usage/revenue/feedback signals fed it) inline,
reusing the provenance/audit infrastructure already specified rather than
building bespoke PM explainability. Jira/Linear/Notion should be v1
(not roadmap) integration targets, wired through the same capability-scoped
`net` grant model in [02-security-core.md §1](02-security-core.md) — the
actual friction users report is a missing *pipe*, not a missing AI feature.

### Technical writers: missing lived-debugging context
AI-generated docs are technically fluent but lack the "anxiety makers go
through" during development — an LLM cannot access why a caveat exists,
only the current code state.

**New feature (added to §08):** the doc-generation pipeline should pull
from **closed bug/incident tickets and PR review comments** referencing the
code path being documented, not just current code + OpenAPI schema — the
closest available proxy for the missing "why this matters, what broke
before" context.

### QA: tests pass but validate nothing (strongest single validation found)
LLM-generated tests average a **20.32% mutation score** on complex
real-world functions — roughly 80% of potential bugs go undetected despite
green CI. Root cause named directly by practitioners: "AI tests validate
what your code does rather than what it should do," writing weak
assertions that would pass even if the feature were broken, and self-healing
selectors "masking real application issues" rather than surfacing
regressions.

**New features (added to §08):** **mutation testing, not coverage
percentage**, as the QA pack's primary quality gate — coverage % is the
wrong metric family entirely per this data; **orthogonal review** applied
to generated tests specifically — route them through a second,
differently-sourced check (an LSP/AST-grounded verification that
assertions exercise the real behavior path, not a stubbed one) rather than
trusting the same model to judge its own test's coverage; any self-healing
selector change is a **reviewable diff in its own right**, never an
invisible auto-repair — this is exactly the mechanism practitioners
describe as hiding real regressions.

## Cross-cutting synthesis

Three failure patterns recurred independently across every audience with
no apparent cross-citation between the communities that reported them —
strong evidence they're structural, not tool-specific:

1. **Silent plausibility beats an obvious error** — the Power BI double-
   count, Julius's confident guessing, Figma Make's generic-but-polished
   output, and QA's "tests pass but validate nothing" are the same failure
   shape wearing five costumes: output that looks correct enough not to
   trigger a second look.
2. **Opacity about "why" erodes trust faster than imperfect output does** —
   every audience section above independently converges on the same fix
   (surface the reasoning/evidence/provenance trail), which is why
   provenance-surfacing should be one shared primitive
   ([02-security-core.md §2](02-security-core.md)) extended to every
   generation surface, not reinvented per audience pack.
3. **"Almost right" costs more to fix than obviously wrong** — near-miss
   output suppresses the very signal (a human's instinct to double-check)
   that would otherwise catch it; the harder case (plausible-but-subtly-
   wrong) needs the UI to surface the *specific point of divergence*
   (which join, which token, which assertion), not a generic confidence
   warning, since generic warnings on near-miss output tend to be
   dismissed precisely because the output "looks fine."

**Implication for the roadmap:** the verification pipeline's provenance-
surfacing, confidence-gating, and orthogonal-review mechanisms
(`02-security-core.md`) are not developer-pack-specific safety machinery —
they are "pro"'s cross-audience differentiator, evidenced independently by
every audience studied, and every audience-pack spec should inherit them
rather than build a parallel trust mechanism.
