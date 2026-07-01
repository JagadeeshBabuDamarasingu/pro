# pro — customer support / support engineers

A mature "support AI" category (Intercom Fin, Zendesk AI, Salesforce
Agentforce, Ada) with billions of deployed conversations — and
correspondingly mature, legally consequential documented failures.

## Landscape and evidence

- **_Moffatt v. Air Canada_** (Canadian Civil Resolution Tribunal, February
  2024): Air Canada's website chatbot gave a customer incorrect
  information about a bereavement-fare refund policy; the airline argued
  the chatbot was "a separate legal entity" responsible for its own words.
  The tribunal rejected this and held Air Canada liable for its chatbot's
  hallucinated policy statement — the standard citation for "an AI support
  agent's hallucination creates real legal/financial liability for the
  company deploying it," parallel to _Mata v. Avianca_ for legal AI (see
  [13-audience-legal.md](13-audience-legal.md)).
- A widely-reported 2025 incident involving **Cursor's own AI support bot**:
  it fabricated a nonexistent "one device per subscription" policy in
  response to a support ticket; the user posted publicly, triggering
  visible backlash and a public correction — notable because it happened
  to an AI-native company itself, underscoring that even sophisticated
  teams get burned by an ungrounded support agent.
- The recurring, unresolved tension in deployed support AI: it confidently
  answers from training data or general web knowledge instead of the
  company's actual current policy — the same "silent plausibility,
  ungrounded in the real source of truth" failure shape documented for
  every other audience in
  [11-user-research-pain-points.md](11-user-research-pain-points.md).

## "pro" design — reuse, not new architecture

1. **graphify grounds every answer in current, versioned policy**, not
   general web knowledge. The knowledge base, current policy documents, and
   historical resolved tickets are exactly the graphify corpus
   ([10-cost-optimization.md §5](10-cost-optimization.md)) — grounding
   directly prevents both the Air Canada and Cursor-bot failure patterns.
2. **Policy-citation grounding** — every customer-facing policy claim must
   resolve to a specific, current KB article ID, the support-domain analog
   of case-citation verification for legal
   ([13-audience-legal.md](13-audience-legal.md)) and package-existence
   verification for developers ([02-security-core.md §3](02-security-core.md)).
3. **Confidence signals/resampling**: if resampled answers to the same
   policy question disagree, escalate to a human rather than answer —
   directly analogous to the Analysts pack's SelfCheckGPT recommendation
   ([07-audience-analysts.md](07-audience-analysts.md)).
4. **Autonomy tiers and blast-radius bounds matter unusually concretely
   here**: support agents routinely have write capability with real
   financial blast radius (issuing refunds, applying account credits,
   cancelling subscriptions). Refund/credit-issuance defaults to a low
   autonomy tier with an explicit **dollar-value blast-radius bound**,
   promoted only with a demonstrated track record — the same staged-
   autonomy model from [02-security-core.md §1](02-security-core.md),
   validated by the same Wiz least-privilege data (17% incident rate vs.
   76% without it) and the Step Finance overprivileged-transfer-agent
   incident already cited there.
5. **Audit log as legal-defensibility asset**: liability cases like Air
   Canada turn on "what did the company's agent actually say and why" — an
   immutable, citation-attached transcript of what the agent claimed and
   which KB article it grounded on is a direct legal-defensibility asset,
   not just an internal safety feature.

## Net-new work

A support-platform connector (Zendesk/Intercom/Salesforce ticket and KB
API) and a refund/credit-issuance capability-grant type with a dollar-value
blast-radius bound — both connector/config work reusing existing
mechanisms, not new architecture.
