# pro — data engineers / ML engineers

Distinct from [07-audience-analysts.md](07-audience-analysts.md) (which is
NL-to-SQL/BI-consumer-facing) and from
[06-audience-devops.md](06-audience-devops.md) (infra/deploy/observability):
this audience owns the pipeline and model artifact itself — schema
evolution, backfills, feature engineering, training-serving skew, data/
model lineage.

## Landscape and evidence

Current tools: dbt (and its AI features), Databricks Assistant, Snowflake
Cortex, Airflow/Dagster-embedded copilots, Great Expectations/Soda for data
quality, MLflow/Weights & Biases for experiment tracking, feature stores
(Tecton, Feast).

- **Lineage blindness is the most-cited operational pain** in dbt/Airflow
  community discussion: a schema change several hops upstream silently
  breaks a downstream model, and generic AI copilots operate only on the
  SQL text in front of them — with no visibility into the DAG's actual
  dependency graph, they routinely suggest fixes that pass locally but
  violate an upstream contract.
- **Training-serving skew** — a feature computed one way in training and a
  subtly different way at serving time — is repeatedly cited (Uber,
  Airbnb, DoorDash engineering postmortems) as the single most common root
  cause of production ML incidents. This is exactly the "silent
  plausibility" failure shape documented across every other audience in
  [11-user-research-pain-points.md](11-user-research-pain-points.md):
  fluent-looking code that's subtly wrong.
- **Data contracts** (originating at GoCardless, formalized in dbt's
  `contracts:` config and the Data Mesh community) exist specifically
  because upstream teams keep breaking downstream consumers with no
  warning — an AI agent editing schemas blind to consumers makes this
  worse; a contract-aware agent fixes it.

## "pro" design — reuse, not new architecture

1. **DAG-grounding** extends the AST/LSP symbol-grounding gate
   ([02-security-core.md §4](02-security-core.md)) to pipeline nodes:
   before an agent edits a dbt model, Airflow task, or feature definition,
   it must resolve the node against the live lineage graph (dbt's
   `manifest.json`, an Airflow DAG parse, or a feature-store registry) —
   the same mechanism as symbol resolution, applied to pipeline nodes
   instead of functions.
2. **graphify is arguably more load-bearing here than for developers.** A
   data platform's schema + lineage + column docs + historical incident
   postmortems is exactly the corpus graphify already targets
   ([10-cost-optimization.md §5](10-cost-optimization.md)) — "what depends
   on this column" is often not discoverable from code text alone, making
   the persistent knowledge graph the primary source of truth rather than
   a cost optimization on top of one.
3. **Confidence signals** apply directly to schema-inference tasks
   (inferring a column's meaning from name/type alone — the same failure
   Julius AI exhibits, per
   [11-user-research-pain-points.md §4](11-user-research-pain-points.md)):
   if resamples disagree on what a column means, flag rather than silently
   proceeding.
4. **New capability-grant types, not new sandbox architecture**:
   `schema-write` and `backfill-execute` as their own scoped, blast-radius-
   bounded grants — blast radius here means downstream tables/rows a
   schema change or backfill could touch — reusing the existing autonomy
   tiers (0-4) and blast-radius bounds from
   [02-security-core.md §1](02-security-core.md) unchanged.
5. **Cost estimator gains a genuinely new dimension.** Backfills and
   full-table scans are exactly the kind of expensive, easy-to-runaway
   operation the pre-flight cost estimate + budget cap
   ([10-cost-optimization.md §7](10-cost-optimization.md)) should gate hard
   before execution — but here the gate must model **warehouse compute
   credits**, not just LLM token cost, which is net-new for the estimator
   to reason about.

## Net-new work

A config/connector layer (dbt manifest parser, Airflow DAG parser,
feature-store registry client) feeding the existing grounding and graphify
pipelines, the two new capability-grant types above, and a warehouse-credit
cost dimension on the existing pre-flight estimator. No new sandbox,
verification, or orchestration architecture required.
