# pro — spec index

"pro" is a productivity suite that gives every technical audience (and several
adjacent ones) a secure-by-default AI layer over their existing tools, built
around a shared Rust core engine. A GUI (for non-technical users) and a
Rust/TS CLI+TUI (for developers) are the two human-facing surfaces; CI
invocation and cloud/background execution are non-interactive modes of that
same core, not separate products.

Read in this order:

1. [00-vision.md](00-vision.md) — problem statement, product pillars, audience map
2. [01-architecture.md](01-architecture.md) — three-surface architecture, framework choices, competitive landscape
3. [02-security-core.md](02-security-core.md) — capability/sandbox model, prompt-injection defense, supply-chain gate, verification pipeline
4. [03-audience-developers-builders.md](03-audience-developers-builders.md)
5. [04-audience-security-researchers.md](04-audience-security-researchers.md)
6. [05-audience-designers.md](05-audience-designers.md)
7. [06-audience-devops.md](06-audience-devops.md)
8. [07-audience-analysts.md](07-audience-analysts.md)
9. [08-audience-additional.md](08-audience-additional.md) — technical writers, QA, PMs, SREs, plus lighter-weight extensions for finance/FP&A, platform engineering, GRC, and localization
10. [09-roadmap.md](09-roadmap.md) — phased build order
11. [10-cost-optimization.md](10-cost-optimization.md) — prompt caching, model cascading, knowledge-graph context ("graphify"), local-model efficiency
12. [11-user-research-pain-points.md](11-user-research-pain-points.md) — evidence-backed pain points per audience (surveys, incidents, forum/review sentiment) and the features they validate or add
13. [12-cloud-task-execution.md](12-cloud-task-execution.md) — cloud/background task execution (à la Claude Code cloud, Cursor Background Agents) as a fourth execution target on the same trust substrate
14. [13-audience-legal.md](13-audience-legal.md) — legal/contract-review: citation-grounding, defined-term/cross-reference verification
15. [14-audience-data-ml-engineers.md](14-audience-data-ml-engineers.md) — data/ML engineers: DAG-grounding, schema/backfill safety, warehouse-credit cost gating
16. [15-audience-support-engineers.md](15-audience-support-engineers.md) — customer support: policy-grounded answers, bounded refund/credit autonomy
17. [16-audience-oss-maintainers.md](16-audience-oss-maintainers.md) — open-source maintainers: AI-slop-scored triage of external contributions

These are brainstorm-stage specs (2026-07-01), informed by research into the
current competitive landscape (Cursor, Windsurf, Zed, Warp, Copilot CLI, Cody,
Continue.dev, Aider, opencode, Amp, Claude Code, Devin, Replit), the
security/agent-architecture literature (Deno permissions, Claude Code/Codex
sandboxing, CaMeL, MELON, PromptArmor), hallucination/quality research
(SelfCheckGPT, semantic entropy, CodeHalu, FrontierCode, GitClear), cost/
inference-efficiency research (prompt caching economics, RouteLLM/FrugalGPT,
LLMLingua, GraphRAG/LazyGraphRAG/cognee, vLLM), and named legal/liability
precedent for the newer audience packs (_Mata v. Avianca_, ABA Formal
Opinion 512, the XZ Utils backdoor, _Moffatt v. Air Canada_). They are meant
to be argued with, not treated as final.
