# pro — additional audiences

Beyond the six named in the original brief, four more plausibly benefit from
"pro" without requiring a fundamentally different architecture — each reuses
capability already planned for other packs. (Four further, stronger-evidence
audiences — legal, data/ML engineers, support engineers, and OSS maintainers
— warranted dedicated specs; see
[13](13-audience-legal.md)–[16](16-audience-oss-maintainers.md).)

## Technical writers / documentation engineers

61% of writing professionals already use AI tools at least sometimes (26%
daily) — the shift is toward high-value editing of AI-drafted content, not
writing from zero. The key differentiator against generic AI writing tools:
ground drafts in *trusted source inputs* (specs, tickets, SME interviews,
OpenAPI schemas, UI copy/labels) rather than free generation — this is the
same grounding/verification principle as
[02-security-core.md §4](02-security-core.md). Concrete "pro" angle: auto-
generate docs directly from a repo's code changes/API surface (reuses the
builder pack's codegen infra, [03](03-audience-developers-builders.md)) and
keep them in sync via CI.

**Validated pain point** (see
[11-user-research-pain-points.md §6](11-user-research-pain-points.md)):
AI-generated docs are technically fluent but structurally cannot capture
the "anxiety makers go through" during development — an LLM only sees the
current code state, not why a caveat exists. The doc-generation pipeline
should pull from **closed bug/incident tickets and PR review comments**
referencing the code path being documented, not just current code + OpenAPI
schema — the closest available proxy for the missing "why this matters,
what broke before" context.

## QA / test engineers

A distinct, mature product category already: **Mabl** (self-healing test
automation), **Functionize** (NLP test generation), **Katalon** (unified
manual+automated+API+perf testing with AI failure analysis), **QA Wolf**
(agentic — generates production-grade Playwright/Appium code from NL
prompts). "pro"'s QA feature set: NL-to-test-script generation, self-healing
selectors, AI-assisted failure triage (real regression vs. flaky test) —
pairs naturally with the builder pack's worktree/best-of-N infra (run
generated tests in an isolated worktree before merging,
[03](03-audience-developers-builders.md)).

**Validated pain point — the strongest single confirmation found across all
audience research** (see
[11-user-research-pain-points.md §6](11-user-research-pain-points.md)):
LLM-generated tests average a **20.32% mutation score** on complex
real-world functions — roughly 80% of potential bugs go undetected despite
green CI. Practitioners name the root cause directly: "AI tests validate
what your code does rather than what it should do," writing weak
assertions that would pass even if the feature were broken, and
self-healing selectors "masking real application issues" rather than
surfacing regressions. Concretely:

- **Mutation testing, not coverage percentage**, is the QA pack's primary
  quality gate — coverage % is the wrong metric family entirely per this
  data, and it maps onto the same "passes tests ≠ actually correct"
  distinction the AI Slop Index already draws from FrontierCode
  ([02-security-core.md §4](02-security-core.md)).
- **Orthogonal review applied to generated tests specifically**: route them
  through a second, differently-sourced check (an LSP/AST-grounded
  verification that assertions exercise the real behavior path, not a
  stubbed one) rather than trusting the same model to judge its own test's
  coverage.
- **Self-healing selector changes are reviewable diffs in their own right**,
  never an invisible auto-repair — this is exactly the mechanism
  practitioners describe as hiding real regressions.

## Product managers

70%+ of PMs reportedly use AI daily already. Reference tools: Perplexity
(cited market research), NotebookLM (interview-transcript synthesis),
Productboard (consolidates feedback across support/sales/interviews,
AI-clusters and scores features), Dovetail (research synthesis), Claude/
Notion AI (PRD drafting). "pro"'s PM feature set: feedback-clustering/
prioritization, auto-generated PRDs from rough notes, roadmap-impact
analysis — can reuse the analyst pack's NL-to-SQL layer for product-
analytics queries ([07](07-audience-analysts.md)) and the designer pack's
prototyping tools for spec-to-clickable-prototype PRDs
([05](05-audience-designers.md)).

**Validated pain point** (see
[11-user-research-pain-points.md §6](11-user-research-pain-points.md)):
AI-drafted PRDs are consistently reported as generic and nuance-blind, and
AI feedback-summarization tends to return "a confident synthesis of what's
already widely known — consensus, not insight," because the compression
step itself discards nuance independent of model quality. PRD generation
must **cite raw source artifacts** (support tickets, call transcripts, NPS
verbatims) inline rather than generating from a summary-of-a-summary — the
same "ground drafts in trusted source inputs" principle stated for
technical writers above, extended explicitly to qualitative PM inputs.
Any AI-suggested priority score must show its inputs (which usage/revenue/
feedback signals fed it) inline, reusing the provenance/audit
infrastructure in [02-security-core.md §1-2](02-security-core.md) rather
than building bespoke PM explainability. Jira/Linear/Notion integration
should ship in v1, not the roadmap — wired through the same capability-
scoped `net` grant model, since the actual reported friction is a missing
*pipe*, not a missing AI feature.

## SREs (distinct from general DevOps)

Called out separately because the 2026 narrative is specifically "AI will
push SRE to its limit" — root-cause correlation across events/logs/metrics/
traces/deploys/config/ownership data, with platforms standardizing telemetry
schemas specifically so agents can act reliably, not just narrate. This
overlaps heavily with [06-audience-devops.md](06-audience-devops.md) (same
read-only-correlation-first, narrow-scope-per-agent principles) —
**open product decision: merge into the DevOps pack, or keep as a distinct
narrower pack** (on-call load reduction, blameless postmortem generation,
error-budget tracking specifically). Lean toward merging unless SRE-specific
demand proves otherwise.

## Finance / FP&A professionals

Distinct from [07-audience-analysts.md](07-audience-analysts.md) in stakes
(outputs feed board decks and external disclosures, under SOX-style
controls) but not in failure shape: the same Power BI Copilot incident
already cited there (a fabricated revenue explanation a CFO repeated on an
earnings call) is FP&A evidence, not just BI-analyst evidence — the
failure raises the stakes from "wrong dashboard" to "material misstatement
risk." Spreadsheet-specific pain: an AI-suggested formula edit that shifts
a relative reference or breaks a named-range dependency across a many-tab
model is a well-known, near-invisible failure class until a downstream
total is wrong.

Rather than a separate pack, ship this as an **extension of the Analysts
pack**: the metric-provenance lookup already specced there
([07-audience-analysts.md](07-audience-analysts.md)) extends to a chart-of-
accounts/GL mapping and forecast-driver glossary; the AST/LSP-style
grounding gate extends to a **spreadsheet dependency-graph check** (parse
the workbook's cell/range dependency graph before accepting any formula
edit, flagging cross-sheet/named-range changes outside the stated target —
the spreadsheet analog of the Designer pack's scope-discipline diffing,
[05-audience-designers.md](05-audience-designers.md)). Net-new: a
spreadsheet formula-graph parser and a GL/ERP connector (NetSuite, SAP,
Workday), not new architecture. The audit log is close to a selling point
here rather than just a safety feature, given SOX's pre-existing
change-attribution requirements.

## Platform engineers / internal developer platform (IDP) teams

Real, enterprise-sales-relevant pain: developers using AI coding assistants
to scaffold services routinely bypass platform-team guardrails entirely
(wrong base image, missing cost tags, wrong network policy) because the
assistant has no visibility into org-defined golden paths (Backstage
templates, OPA/Rego policies) — the same "92% lack visibility / only 16%
govern access effectively" gap already quantified for DevOps
([11-user-research-pain-points.md §2](11-user-research-pain-points.md)),
seen from the platform team's side instead of the individual developer's.

This is best framed as an **org-admin control-plane view over infra
already built**, not a dedicated pack with new agent behavior: the
capability inventory view and autonomy tiers
([02-security-core.md §1](02-security-core.md)) aggregated across an org so
a platform team can set org-wide ceilings (e.g., no agent gets tier-3+
autonomy on `terraform apply` against prod, org-wide, regardless of what an
individual developer grants), plus golden-path grounding (resolve a
scaffold request against real Backstage templates/OPA policies before
generating, rather than a generic prompt) and blast-radius bounds declared
at the platform-template level (max monthly spend, max resource count).
Worth building as an enterprise-sales/CISO-buyer hook even though it isn't
its own roadmap line.

## GRC (governance, risk, compliance) officers

The thinnest of the newly-identified audiences in terms of net-new agent
capability required — almost the entire value is **"pro"'s own audit
log and capability inventory, exported as a compliance artifact**, not a
new workflow. The risk this audience specifically worries about: an
AI-generated compliance-evidence narrative claiming a control is satisfied
when the underlying evidence doesn't actually demonstrate it — a direct
liability exposure (a failed audit or false attestation), and a frequently-
raised concern as compliance-automation platforms (Vanta, Drata,
Secureframe) add more generative features.

Ship as a **reporting/export feature**, not a dedicated pack: the
three-way audit causal classification and capability inventory
([02-security-core.md §1](02-security-core.md)) are, largely unmodified,
the evidence artifact a GRC officer needs for "how do you govern your own
AI agents' access" audit questions (a growing SOC 2/ISO 27001 line item).
graphify grounds control-mapping narratives in the org's actual written
policy text and prior audit findings rather than the model's generic sense
of what a control typically means — the same grounding pattern as the
legal pack ([13-audience-legal.md](13-audience-legal.md)), applied to
policy text instead of contracts.

## Localization / internationalization (i18n) teams

Real pain, clean graphify fit, narrow scope. Raw LLM translation is fluent
but inconsistent with a product's established terminology/glossary — the
same "silent plausibility" failure shape as everywhere else in
[11-user-research-pain-points.md](11-user-research-pain-points.md), applied
to translation-memory consistency instead of code or contract terms.
Short UI strings ("Open" as verb vs. adjective) are a well-known source of
context-loss errors current MT can't resolve from the string alone.

Best absorbed as a **feature within the Technical Writers pack** above
rather than its own line: the translation memory + terminology glossary +
UI-string context is a graphify corpus like any other, and the same
grounding-gate pattern (verify every translated string against the
glossary's approved term, flagging deviation as a reviewable diff) applies
directly. Confidence/resampling flags ambiguous short strings for mandatory
human linguistic QA rather than auto-publishing. Little sandbox/cost-
optimization surface beyond what's already built — the workflow is almost
entirely grounded generation plus review.

## Explicitly out of scope: no-code builders / citizen developers

Distinct from the "Builders/Developers" audience already scoped (which is
technical: agent orchestration, local models, worktrees). A no-code audience
(Bolt.new/Uizard/v0-style "describe it, get a working app" with no terminal
involved) is a different product — "pro" targets technical and
semi-technical roles operating through a real CLI/TUI or GUI, not a
zero-code app builder. Noted here explicitly to avoid tool-name confusion
(v0, Uizard) between the Designer pack and this out-of-scope segment.
