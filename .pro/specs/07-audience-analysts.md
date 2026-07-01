# pro — analysts

## Landscape

Four current lanes: **chat-first** (ChatGPT Advanced Data Analysis/Code
Interpreter, Claude — shows the executed Python, which builds trust by
making computation auditable); **notebook/workspace** (Hex, Deepnote — SQL +
Python/R, publishable interactive "data apps"); **spreadsheet/NL-first**
(Julius AI — connects directly to Postgres/Snowflake/BigQuery/CSVs, answers
plain-English questions against *live* data, auto-charts, scheduled
auto-updating reports, explains each analytical step; Akkio for no-code
predictive/ML); **warehouse/BI-native** (Gemini in BigQuery, Power BI
Copilot, ThoughtSpot Spotter — AI embedded in the BI tool the company
already has).

## NL-to-SQL: the real differentiator is schema-awareness, not the model

Frontier LLMs alone get ~70-85% accuracy on clean, well-documented schemas,
dropping to 50-70% on messy enterprise databases with unclear naming and no
context. A proper **semantic layer** (business-context schema descriptions,
defined metrics/glossary) pushes this to 86-95%. The differentiator versus
"just ask ChatGPT" is connecting to the real database and understanding
actual table/column relationships — not general SQL knowledge.

For regulated/finance-adjacent teams, a **governed metrics layer** (every
generated query resolves through centrally-defined business logic — e.g.
"revenue" always means the same computed thing) is required, not optional.
Named reference tools: AI2SQL, Vanna, Defog SQLCoder, Snowflake Cortex
Analyst, Seek AI. State of the art is moving toward RL-against-execution-
feedback training (Snowflake's Arctic-Text2SQL-R1, 71.83% on BIRD) rather
than static supervised fine-tuning — worth tracking, not a v1 requirement.

## "pro" design

- **CLI/TUI**: NL-to-SQL execution against connected warehouses (the same
  Steampipe-style SQL abstraction from the DevOps pack,
  [06-audience-devops.md](06-audience-devops.md), can cover non-database
  sources too), scriptable/scheduled report generation (cron-able, Julius-
  style auto-updating reports), pipeable JSON/CSV output for downstream
  processing.
- **GUI**: chart/visualization rendering, notebook-style interactive
  exploration (Hex-like), shareable published "data app" views for
  non-technical stakeholders — this is where CLI-only competitors
  (Steampipe, kubectl-ai) fall short and where the hybrid architecture is a
  real advantage.
- **Governed metrics layer required from v1** if any regulated/finance use
  case is in scope — don't let the model reinvent metric definitions
  per-query.
- **v2 direction**: move from reactive Q&A toward an autonomous "watch this
  metric and alert/explain deviations" reporting agent, once the read-only
  NL-to-SQL layer is solid — mirrors the DevOps pack's read-only-correlation-
  first principle.

Finance/FP&A is a distinct-stakes extension of this pack (GL/ERP-grounded
metrics, spreadsheet dependency-graph checks, SOX-relevant audit trail) —
see [08-audience-additional.md](08-audience-additional.md).

## Validated pain points: silent wrong answers

This is the best-documented failure mode in the analyst space — see
[11-user-research-pain-points.md §4](11-user-research-pain-points.md) for
full sourcing. A real, concrete incident: Power BI Copilot answered a
different, easier question than the one asked, producing a fabricated
15%-enterprise-driven revenue explanation for a spike actually caused by a
join fan-out double-counting three transactions — **a CFO repeated the
false figure on an earnings call** before anyone caught it. Microsoft's own
documentation admits the output is nondeterministic. Julius AI reviews
describe the same mechanism: "if a database has poorly named columns,
Julius will confidently guess their meaning, fabricating insights if users
aren't closely validating." Enterprise-realism benchmarks (Spider 2.0) show
models scoring 90%+ on clean schemas collapse to 10-20% on realistic
enterprise complexity, and critical metric definitions ("Churn Rate")
typically live in external documentation the schema itself never exposes.

- **Metric-provenance lookup as a mandatory pre-step**: any term mapping to
  a business metric must resolve against the governed metrics-glossary
  before SQL is generated, logged the same way capability grants are logged
  in [02-security-core.md §1](02-security-core.md) — so an auditor can
  later see exactly which glossary entry a number resolved through, and the
  join-fan-out class of failure becomes visible before the answer ships.
- **SelfCheckGPT-style resampling as a cheap nondeterminism check**: run
  the same NL query 3-5 times; if resamples disagree, flag rather than
  silently present one answer as ground truth — directly targets the
  nondeterminism Microsoft's own docs admit to.
- **The executed query/code view is non-optional**, always visible
  alongside the metric provenance, never a power-user toggle — chat-first
  tools build trust specifically by showing their work, and auditability
  (not accuracy claims alone) is what the evidence shows actually earns
  trust here.
