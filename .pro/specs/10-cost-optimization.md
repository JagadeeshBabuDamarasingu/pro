# pro — cost optimization

Cost optimization is a cross-cutting concern in the shared Rust core, not a
per-audience feature. **Hard constraint: no cost optimization may bypass or
weaken the capability/sandbox system or the verification/de-hallucination
pipeline** ([02-security-core.md](02-security-core.md)) — every technique
below sits upstream of those gates, never in place of them. Where a
technique creates tension with that constraint, the section says so
explicitly.

## Priority order

1. **P0 — Stability-ordered prompt assembly + provider prompt caching.**
2. **P0/P1 — Structured output as the default for internal agent steps +
   per-task-type token budgeting.**
3. **P1 — Model cascading, using the existing verifier as the escalation
   signal.**
4. **P1 — Static dependency-graph context (Aider-style repo map) as the
   default codebase context mechanism.**
5. **P2 — Incremental, hash-gated knowledge graph** (the "graphify" layer —
   see below) **layered on top of the dependency graph.**
6. **P2 — Local-model quantization + Ollama/llama.cpp integration** for
   local-dev mode.
7. **P2 — Scoped semantic/result caching**, narrow scope, verification
   always still runs on a cache hit.
8. **P1 (only once a self-hosted multi-user server ships) — continuous
   batching + paged KV-cache** (vLLM-class engine).
9. **P3 — Generic token-compression (LLMLingua-class)**, held for last
   because of integration risk with the verifier (see §4).

## 1. Prompt/context caching (provider-side)

Anthropic, OpenAI, and Gemini all offer prefix-based prompt caching at
~75-90% discount on cached reads, but with materially different mechanics:
Anthropic uses explicit `cache_control` breakpoints (max 4/request, 5-min or
1-hour TTL, strict byte-prefix match, 20-block lookback per breakpoint);
OpenAI caches automatically once a prompt exceeds 1024 tokens, no manual
markers; Gemini requires an explicit `CachedContent` object and — unlike the
other two — charges an ongoing **storage fee** while the cache is held idle,
so an unused Gemini cache costs money even if never read (best suited to a
few large, known-stable contexts, not many small per-session caches).

**Design patterns that maximize hits, regardless of provider:**

- Freeze the system prompt — never interpolate timestamps, session IDs, or
  per-user data into position-0 content; push anything dynamic to the end
  of the message list.
- Append-only conversation structure — never edit/reorder past turns.
- Deterministic serialization — sorted JSON keys, sorted tool lists, never
  serialize from a `set`.
- Never hot-swap the model or tool list mid-session — both are position-0
  renders and invalidate the entire cache.
- Subagent/fork calls must copy the parent's exact `system`/`tools`/`model`
  prefix verbatim, then append fork-specific content.

**"pro" design:** implement a single **stability-ordered prompt builder** in
the Rust core — `tools → frozen system preamble → stable long-lived context
(repo map / retrieved subgraph) → session-scoped context → per-turn volatile
content` — with cache breakpoints inserted programmatically at each
stability boundary. Build this once in the core so every surface (GUI/CLI/
TUI) benefits identically; never let call sites hand-assemble prompt
strings. Implement provider-specific caching behind a common trait so
Anthropic's inline markers, OpenAI's automatic caching, and Gemini's
explicit cache-object lifecycle (including proactively deleting caches
`pro` created, to avoid Gemini's idle storage fee) are all handled
correctly. Surface `cache_read`/`cache_creation`/`input` token counts per
session in a `pro cache stats` command (and a TUI/GUI panel) so silent
cache-invalidators are debuggable, not invisible.

## 2. Semantic/result caching

Application-layer caching that matches on *meaning* (embed the query, ANN
search against previously-answered queries, return the cached response
above a similarity threshold) rather than exact prefix — GPTCache is the
reference open-source implementation; reported production hit rates of
60-70% with >97% positive-hit accuracy are achievable, but the risk profile
is real: a false-positive "near-miss" hit on two semantically similar but
materially different questions (e.g. "is port 8080 open on prod" vs.
"...on staging") returns a **confidently wrong** answer, and a cache keyed
on untrusted content is a fresh cache-poisoning vector.

**"pro" design:** scope semantic caching to stable, generic,
non-state-dependent queries only ("explain this error," "what does this
flag do") — never to anything inspecting live system/file/process state.
**Mandatory rule: a semantic-cache hit still passes through the full
verification pipeline** — caching skips the generation call, never the
check. Flag cache-served responses in the UI so a professional user can see
when an answer wasn't freshly reasoned about their specific input.

## 3. Model routing / cascading

Two distinct mechanisms: **routing** (RouteLLM — classify the query up
front, send to the cheapest sufficient model tier; reports 85% cost
reduction retaining 95% of top-model quality) and **cascading** (FrugalGPT —
run the cheap model first, use a confidence/correctness signal to decide
whether to escalate to a more expensive model; reports matching best-model
accuracy at up to 98% cost reduction in benchmarked settings). Escalation
signals used across the field: self-consistency disagreement, token
logprobs, trained confidence classifiers, or — the option most relevant
here — **an independent verifier's pass/fail result**.

**"pro" design:** adopt a two-stage cascade, not routing alone:

1. Lightweight up-front classification sends trivially easy tasks (simple
   lookups, syntax questions, single-file formatting) straight to the
   cheapest tier (local model or a small hosted model).
2. For everything else, run the fast/cheap tier first, then run its output
   through the **existing verification/de-hallucination pipeline** — treat a
   verification failure as the escalation signal. This reuses infrastructure
   "pro" already has instead of training a bespoke correctness classifier.

Log every cascade decision (model tried, verification result, whether
escalation occurred) for auditability — this matters more for a
professional/security-adjacent tool than a consumer chatbot. Cascading
governs *which model answers*, never *whether verification runs*, and must
never be used to route around a capability/permission check.

## 4. Context compression / token reduction

General-purpose prompt compression (LLMLingua-2 — a small BERT-scale
encoder trained via GPT-4 data distillation to classify tokens as keep/drop,
2-5x compression, 1.6-2.9x latency improvement, task-agnostic) is real but
carries integration risk: a token judged "low information" by a proxy model
may be exactly the detail the verification pipeline needs to check a claim
against. **Scope any adoption of generic compression to background/reference
context only (older turns, docs) — never the primary evidence (current
diff, current command output) that the verifier checks claims against.**
This is a P3 item, held for last.

**Coding-agent-specific context management is different and higher
priority.** Aider's repo map — a `tree-sitter`-derived directed graph over
the codebase (files as nodes, symbol-reference edges), ranked by personalized
PageRank, rendered as an elided, scope-aware view within a fixed token
budget — is effectively **knowledge-graph retrieval with zero LLM indexing
cost**. Claude Code's `/context`/`/compact` model (context *editing* —
pruning stale tool results, cheaper and lossless — preferred over full
*summarization*, which loses detail; both are cheaper than hitting the
context wall) is the reference for session-level context management.

**"pro" design:** adopt the Aider-style dependency graph (via `tree-sitter`,
Rust-native) as the **default, always-on** codebase-context mechanism —
this is "GraphRAG-lite," free at query time, and the natural first rung of
the knowledge-graph ladder in §5. Implement `/context`-equivalent
introspection and a manual-compact command across all three surfaces;
prefer pruning stale tool-call results over summarization wherever the
budget allows, and intervene proactively around ~60% context utilization
rather than waiting for the wall (models recall the full conversation
better earlier, producing better summaries when compaction does happen).

## 5. Knowledge-graph-based context retrieval — the "graphify" layer

This is the deepest architectural investment in this document, and the one
most directly tied to the `graphify` concept (turning a corpus — codebase,
docs, papers — into a persistent knowledge graph once, then querying
targeted subgraphs instead of re-sending raw text every request).

**The expensive part of GraphRAG is LLM-driven summarization at indexing
time, not entity extraction itself.** Full Microsoft GraphRAG (entity/
relationship extraction → community detection → LLM-generated hierarchical
community summaries) has a documented indexing-cost problem — one widely
cited case went from $33,000 to index a dataset down to roughly 0.1% of
that through architecture changes alone, and summary hierarchies grow
super-linearly with corpus size, which is the real obstacle to cheap
incremental updates. **LazyGraphRAG** fixes this by deferring summarization
to query time: indexing does only cheap graph construction, and
summarization/reasoning happens lazily, scoped to the subgraph a query
actually touches — reported indexing cost near plain-vector-RAG levels,
with local-query quality matching or beating full GraphRAG.

**Hallucination-reduction evidence** (this connects directly to
[02-security-core.md §4](02-security-core.md)): graph-grounded retrieval
follows explicit entity/relationship edges rather than embedding-distance
proximity, which targets exactly the failure mode where a vector retriever
pulls in a plausible-looking but structurally unrelated chunk that the model
then extrapolates from. Benchmarked gains concentrate on **multi-hop
queries** ("what calls this function, which of those call sites are in the
auth path, which changed in the last release" — the actual profile of most
non-trivial coding-agent questions) — a counterpoint worth taking seriously
("UnWeaving the Knots of GraphRAG") argues well-tuned vector RAG closes most
of the gap on simple lookups, so the practical answer is **hybrid**: vector
retrieval for "find similar code," graph retrieval for "find related/
dependent code."

**Incremental updates**: `cognee`'s pattern — content-hash each file at
ingest, reprocess only changed/new files on re-run, explicitly designed to
run as a CI step on every merge — is the model to follow for keeping a
codebase graph continuously synced without full re-indexing. Full
re-indexing should only ever be triggered by a change to chunking/extraction
parameters, never routinely.

**"pro" design, staged:**

1. Start with the free tier already specified in §4 (Aider-style static
   dependency graph, zero LLM cost) as the always-on default.
2. Layer an incremental, hash-gated **semantic** graph on top for reasoning
   that needs entity/relationship linkage beyond pure code-dependency
   structure (e.g., linking a security finding → the function it affects →
   the ticket that tracks it → the commit that introduced it). Follow
   cognee's ingestion pattern (content-hash gating) whether built on cognee
   directly or reimplemented natively against a Rust-native graph store.
3. Adopt the **LazyGraphRAG philosophy** (defer summarization to query time)
   for any corpus-wide query ("what does this codebase do," "summarize this
   security posture") — precomputing community summaries for a fast-changing
   local repo is the expensive, poorly-amortized case the research
   explicitly warns against; every summary goes stale the moment the code
   changes.
4. Store the graph in an embedded, Rust-native store for the local-dev
   single-user case (avoid requiring a Neo4j server for the default
   experience); support Neo4j as the backing store for a self-hosted
   multi-user server deployment where operators likely already run it.
5. Full re-index is an explicit, rare, user-triggered action ("rebuild
   index"), distinct from the default incremental sync.
6. **Every graph node/edge carries provenance (file/commit/finding) as
   first-class metadata, all the way through to the verification pipeline**
   — a knowledge graph is exactly the kind of denser, pre-summarized context
   where it's easiest for a de-hallucination check to lose the thread back
   to ground truth if provenance isn't preserved structurally.

## 6. Local-model efficiency (batching, quantization, speculative decoding)

Quantization (GGUF Q4_K_M as the default balance of quality/footprint, with
an importance matrix to reduce perplexity loss 10-30% over naive
quantization), speculative decoding (a small draft model proposes tokens,
the target model verifies in parallel — 1.6-3x speedup), continuous
batching, and paged KV-cache management (vLLM's PagedAttention — up to 24x
throughput at high concurrency) all matter, but **for different deployment
shapes**:

| Technique | Single-developer local-dev | Self-hosted multi-user server |
|---|---|---|
| Quantization | Essential | Matters less (memory less constrained) |
| Speculative decoding | High value (latency-bound) | Competes with batching for GPU resources |
| Continuous batching | No value (one request at a time) | Essential |
| Paged KV-cache | Marginal | Essential |
| Prefix/KV-cache reuse | Valuable | Valuable, same class as §1 |

**"pro" design:** integrate with **Ollama** as the primary local-model
backend for the default local-dev experience (don't reimplement quantization
or model management — Ollama already wraps llama.cpp well); default to
Q4_K_M-class quantization; treat speculative decoding as opt-in/advanced,
not default (needs a compatible draft-model pair, nontrivial to auto-select).
If/when "pro" ships a self-hosted multi-user server mode, put **vLLM**
(or an equivalent continuous-batching engine) behind that API instead of
llama.cpp — this is a distinct code path, not a superset of local-dev mode.
Apply the §1 stability-ordered-prompt discipline to local-model prompts too;
llama.cpp/vLLM prefix-caching gives the same class of savings as hosted
caching. **The local-model code path must enforce the same capability/
sandbox gating as the hosted-API path** — "runs locally" must never become
an implicit excuse for broader filesystem/network access.

## 7. Output-side cost control

Output tokens are consistently 3-8x more expensive than input tokens across
providers, so output-side discipline has outsized leverage. Techniques:
per-task-type `max_tokens` ceilings (not one global default); streaming with
early-stop on a structural completion marker; structured output/function-
calling instead of free-form prose (both cheaper in tokens and, usefully,
easier for a mechanical verifier to check than prose); explicit concision
prompting backed by a hard token ceiling; provider-specific effort/verbosity
controls (e.g. Anthropic's `effort` parameter) as the primary per-request
dial.

**"pro" design:** make structured tool-calling/JSON the default output mode
for every *internal* agent step (plan generation, tool selection, diff
proposals), reserving free-form prose for the final user-facing summary —
this is the one recommendation in this whole document where the cost
optimization and the correctness goal point in the same direction, since
structured output is exactly what the verification pipeline can check
mechanically (schema validation, field cross-referencing) far more reliably
than prose. Set per-task-type token ceilings in the core's task-type
registry. Default subagent/tool-selection calls to low/medium effort,
reserving high effort for the primary task-solving call — this maps
directly onto the cascade design in §3 (cheap-tier calls also run at low
effort). Never let terseness prompting suppress the specific evidence
(file:line, command output) the verifier needs — scope aggressive
concision to user-facing prose only, never to the structured evidence
trail.

### Pre-flight cost estimation and hard budget caps

This is the direct fix for a documented, concrete failure: Cursor silently
moved Pro users from a flat-fee plan to raw-API-rate billing, producing
bills like "$350 in overage in a week" with no warning, forcing a public
apology and refunds (see
[11-user-research-pain-points.md §1](11-user-research-pain-points.md)).
Per-task-type token ceilings (above) bound a single step's cost, but don't
by themselves prevent a long multi-step agentic task from silently
accumulating an unexpectedly large total bill across many steps.

**"pro" design:** before executing a multi-step task (and before every
escalation in the cascade in §3), estimate total token/compute cost from
the planned step count and each step's ceiling, and require explicit
confirmation once the running estimate crosses a user-configurable
threshold — default threshold set conservatively, always overridable per
session. Surface the live estimate alongside the `pro cache stats`-style
token-count reporting already specified in §1, so cost visibility is one
consistent UI element (GUI panel, TUI status line, CLI `--json` field)
rather than a separate feature. This applies identically to the cloud
execution target ([12-cloud-task-execution.md](12-cloud-task-execution.md)),
where the "close your laptop and let it run" pitch makes an unbounded
running estimate the same failure mode at higher stakes.
