# Research note: confidence signals over an HTTP-only provider abstraction

Written while scoping `pro-confidence` (Phase 2). Question that prompted
this: `02-security-core.md` §4 names **Semantic Entropy Probes** (arXiv:
2406.15927, Kossen et al. 2024) as the preferred confidence signal "where
hidden states are available," with SelfCheckGPT-style sampling consistency
as the black-box fallback. `pro`'s `Provider` abstraction
(`packages/core/crates/pro-provider`) only ever talks to an OpenAI-compatible
HTTP endpoint — it never holds model weights or activations. Does that put
Semantic Entropy Probes permanently out of reach, or is that just an
unbuilt integration?

## The finding: it's a real ceiling, but only for the *Probe* variant

There are three distinct things in this family, with very different
requirements:

1. **Semantic Entropy Probes (SEPs)** — Kossen et al. 2024. A lightweight
   linear probe *trained* on a single generation's hidden-state activations,
   predicting what full semantic entropy (see #2) would have measured. The
   entire point of the technique is near-zero added latency: one forward
   pass, no resampling. This requires two things no HTTP completions API
   exposes: (a) the hidden states themselves, and (b) a training pass to fit
   the probe in the first place. **Not achievable over `pro-provider` as
   designed, and not achievable over *any* OpenAI-compatible HTTP surface**
   — this isn't specific to Ollama or any one backend, it's a property of
   what a completions endpoint returns. The only way to get this signal
   would be to embed a local inference runtime directly in the Rust process
   (e.g. `candle` or `llama-cpp-rs` bindings, loading weights in-process
   instead of shelling out to an HTTP server) — a materially different,
   heavier architectural commitment than the "one HTTP client, many
   backends" design `01-architecture.md` deliberately chose. Treated here as
   a considered-and-deferred fork, not a bug or a missing feature.

2. **Full Semantic Entropy** — Kuhn et al. 2023 ("Semantic Uncertainty"),
   Farquhar et al. 2024 (Nature). Sample N full generations at nonzero
   temperature, cluster them by *meaning*-equivalence (bidirectional NLI
   entailment between pairs — "do these two answers assert the same
   claim?"), then take Shannon entropy over the resulting cluster-size
   distribution. High entropy over meaning clusters (not surface phrasing)
   is the confabulation signal. **This needs no hidden states at all** — it
   only needs repeat sampling (which `pro-provider` already supports) and an
   entailment judge, which can be approximated with a structured-output LLM
   call through the same `Provider` trait already used for everything else.
   The SEP paper's whole contribution *is* replacing this multi-sample
   process with a single-pass probe — so the thing SEPs approximate is
   itself fully buildable here, just at the sampling cost SEPs were invented
   to avoid.

3. **SelfCheckGPT-style sampling consistency** — Manakul et al. 2023. Also
   N resamples, but clusters/scores by lexical or surface-level agreement
   rather than semantic (NLI) equivalence. Weaker signal than #2 (can be
   fooled by consistent-but-wrong phrasing, or miss agreement expressed in
   different words), but zero extra API calls beyond the N samples
   themselves — no judge call needed.

4. **Logprob-based proxy** — some inference servers return per-token
   `logprobs`/`top_logprobs` on chat completions even without exposing
   hidden states. This is *not* semantic entropy (it's token-level
   predictive probability, conflates lexical surprise with real semantic
   uncertainty) but it's a legitimate, much cheaper, single-generation
   signal when available.

## What's actually available in this environment (verified, not assumed)

Tested directly against the local Ollama instance already used for the
Phase 0 `pro ask` demo:

```
curl http://localhost:11434/v1/chat/completions \
  -d '{"model":"functiongemma:latest","messages":[...],"logprobs":true,"top_logprobs":2}'
```

Confirmed the response includes a full `choices[0].logprobs.content[]`
array with per-token `logprob` and `top_logprobs`. So signal #4 is real and
usable here today, not hypothetical — though it's backend-dependent (some
OpenAI-compatible servers omit `logprobs` entirely), so it has to degrade
gracefully to `None` rather than being relied on.

## Resulting design for `pro-confidence`

Build all three *achievable* signals behind one `ClusteringStrategy`
abstraction over a shared `sample_n` (reusing the existing `Provider` trait
— no new provider machinery needed):

- `LexicalConsistency` (#3) — the cheap default, no judge call.
- `EntailmentClustering` (#2) — the strong signal, real full Semantic
  Entropy, at the cost of an extra structured-output judge call per pair.
- `logprob_confidence` (#4) — opportunistic, zero extra sampling, `None`
  when the backend doesn't return it.

**Do not build a stub for Semantic Entropy Probes.** Unlike Phase 0's
`ShadowEnforcer` (a real, complete implementation of one tier of a
multi-tier system that will gain more tiers later), there is no planned
in-process local-inference backend to eventually fill this gap — adding one
would be a distinct, heavier architectural decision (embedding model weights
in the Rust binary) that hasn't been proposed or scoped anywhere in
`.pro/specs/`. A trait with no implementation and no near-term path to one
would just be dead code pretending to be a roadmap item. If in-process local
inference is ever seriously considered (e.g. to also improve the Phase 0
sandbox story, or for genuinely offline/air-gapped use), SEPs become a
natural addition at that point — worth a forward-reference here, not a
stub now.
