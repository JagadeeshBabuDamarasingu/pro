# pro — DevOps engineers

## Landscape

**k9s** is the UX reference for a TUI-first DevOps tool — keyboard-driven,
real-time resource views, log tailing, narrow and excellent at one thing.
**kubectl-ai** layers NL over kubectl with a pluggable multi-provider config —
good precedent for "pro"'s own provider abstraction. **k8sgpt**/**kagent**
round out the current AI-K8s-troubleshooting trio.

**Komodor's KlaudiaAI** is the key product lesson: scoping the agent
narrowly to root-cause analysis (not general chat) is explicitly what made
it reliably valuable — a lesson "pro" should apply across every DevOps
feature, not just incident response.

**Datadog Bits AI (SRE)** sets the bar for action-taking (not just
diagnosing): a fixed menu of ~7 typed triage actions (Slack/Teams message,
open incident, create Jira/case) — each independently auditable, not
free-form execution.

**Steampipe** exposes 150+ cloud/SaaS APIs as SQL tables via an embedded
Postgres engine, zero-ETL (live API calls per query) — this is a directly
reusable architecture: "ask a natural-language question about your infra"
becomes NL → SQL → Steampipe-style plugin query, rather than bespoke API
integration per cloud/tool.

**Spacelift Intelligence** is the most mature "agentic IaC" reference: an
Infra Assistant (conversational, full context on stacks/state/runs), Intent
(NL provisioning, restricted to non-prod), and automatic RCA on failed IaC
runs — IaC-agnostic (Terraform/OpenTofu/Pulumi/Ansible/CloudFormation),
BYO-LLM. **AWS Q Developer is being sunset (2026) in favor of Kiro** — treat
Kiro, not Q, as the current AWS reference point.

## "pro" design

1. **Read-only correlation layer first** — the valuable work is correlating
   signals engineers already inspect manually (events, logs, metrics,
   traces, deploys, config changes, git changes, alerts, on-call/ownership
   data) into one narrative, not generating new telemetry.
2. **Narrow, verifiable diagnostic scope per agent** (the Komodor lesson) —
   root-cause a failed deployment, explain a CrashLoopBackOff — never one
   open-ended DevOps chatbot.
3. **Typed, fixed action menu for anything side-effecting** (Bits AI SRE
   pattern) — page, message, open ticket/case — each a distinct auditable
   tool call, never raw shell execution for production actions.
4. **SQL-as-universal-interface** (Steampipe pattern) for querying
   heterogeneous cloud/observability sources — reuses the same NL-to-SQL
   techniques as the analyst pack ([07](07-audience-analysts.md)).
5. **Provisioning guardrail split** (Spacelift's model): AI can generate/act
   freely against non-production environments; anything touching production
   IaC goes through normal human-reviewed diff review — never direct AI
   apply.
6. **CI-safe by construction**, not retrofit: deterministic flag/env/config
   invocation (no required interactive prompts), `--json` structured output
   as first-class, meaningful non-zero exit codes, a wizard-fallback only
   when stdin is a real TTY (fail fast otherwise), token/API-key auth (no
   OAuth browser flows) for headless pipeline use. This is the same
   dual-mode requirement as [01-architecture.md](01-architecture.md), applied
   here to deployment/observability workflows specifically.

## Validated pain points and required guardrails

Field data sharpens the design above — see
[11-user-research-pain-points.md §2](11-user-research-pain-points.md) for
full sourcing.

- **Baseline-exclusion is mandatory, not a nice-to-have.** Deployments
  trigger 20+ false-positive anomaly alerts for ~2 hours post-deploy
  because models trained on pre-deploy baselines flag new-but-normal
  behavior; worse, a model trained on a window that included a past outage
  learns to treat that outage's error rate as "normal." Engineers must be
  able to explicitly tag and exclude known-outage windows and post-deploy
  stabilization windows from anomaly-training data, itself audit-logged as
  a first-class config object in `pro.config.mjs`.
- **Measure the verification tax, don't assume net time savings.** Toil
  rose to 30% industry-wide (from 25%, the first rise in five years)
  *despite* 51% of organizations already deploying AI agents, and 69% of
  AI-powered decisions still require human verification. Log both
  time-to-suggest and time-to-verify for every AI-suggested action,
  surfaced per capability, so a team can see empirically whether a given
  automation is net-time-positive rather than assumed to be.
- **Precision over recall for RCA, by design, not just by model tuning.**
  incident.io's data: at 70% precision, 3 of 10 RCA suggestions waste
  investigation time during an active incident; at 50% precision the
  system is "essentially random," and false positives destroy trust faster
  than false negatives (which are recoverable via continued investigation).
  Every RCA suggestion ships as a **structured evidence chain** (hypotheses
  checked → evidence per hypothesis → confidence → what was *not* checked
  and why), never a bare conclusion, with low-confidence hypotheses
  suppressed behind an explicit "show more" expand rather than shown
  ranked alongside high-confidence ones.
- **"Zero new persistent UI surface."** 51% of organizations cite tool
  sprawl/siloed views as their top operational challenge, and engineers
  lose 6-15 hours/week navigating an average 7.4 tools. Deliver correlated
  findings into existing surfaces (Slack, the existing ticket system) by
  default — never require a new dashboard to remember to check.
- **Staged autonomy as the default, not an opt-in safety feature.** The
  autonomy-tier schema and blast-radius/reversibility gating specified in
  [02-security-core.md §1](02-security-core.md) apply directly here: every
  DevOps capability defaults to shadow mode (tier 0), with promotion to
  autonomous execution requiring a demonstrated observation period and
  explicit sign-off — the industry-standard "100% shadow weeks 1-4, promote
  one low-risk/high-volume category in week 5" pattern, not a policy
  document teams must separately enforce.

## Overlay, not replacement

Real DevOps shops run 2-3 tools together already (paging + observability +
an AI layer). Design "pro"'s DevOps pack as an **aggregation/overlay layer
across existing tools** (Kubernetes, cloud providers, Grafana/Prometheus,
Datadog, PagerDuty, GitHub) rather than a replacement for any one of them —
this is the emerging pattern in cross-tool agents (e.g. IncidentFox's
300+-integration model).

Platform engineering/IDP teams are an org-admin extension of this pack's
capability inventory and autonomy tiers (org-wide autonomy ceilings,
golden-path grounding against Backstage/OPA) — see
[08-audience-additional.md](08-audience-additional.md).
