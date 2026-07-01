# pro — security researchers

## Landscape

**PentestGPT** (arXiv 2308.06782, USENIX Security 2024 Distinguished
Artifact) is the foundational academic pattern: three self-interacting
modules dividing reasoning/generation/parsing so the agent doesn't lose
overall engagement context across a long test — 228.6% task-completion
improvement over raw GPT-3.5. **Pentest Swarm AI** (open source, Go +
Claude API) uses a multi-agent **stigmergy** pattern — agents coordinate via
a shared blackboard of findings with decaying "pheromone weights" rather
than a central planner — and ships native ProjectDiscovery tool integration
plus **scope validation built into the Nmap adapter itself**. Commercial/
research entrants (Penligent.ai, Simbian, BreachSeek, AutoPentest,
PentestAgent) converge on the same shape: classic tools (Nmap, Nuclei, Burp,
Metasploit, SQLmap, BloodHound) with an AI reasoning/orchestration layer on
top, not full tool replacement.

## Responsible-use guardrails (observed in the field, required here)

- **Scope validation at the tool-adapter level** — the agent should be
  structurally unable to scan/exploit outside a declared target scope,
  regardless of what it's asked to do (Pentest Swarm's model).
- **Network-layer allow-listing of the scope** — enforced independent of the
  model's own judgment, so a compromised or prompt-injected agent can't
  pivot to out-of-scope or production infrastructure (this is the
  documented "scope drift" failure mode).
- **Authorization gate** before exploitation-class tools (Metasploit)
  unlock, distinct from passive/recon-class tools (Nmap, Nuclei) — e.g.
  requiring a signed engagement/scope document or a verified role, since
  safety fine-tuning alone is not robust against a jailbreak framed as a
  legitimate pentest request.
- **Immutable audit logging** of every tool invocation and finding — this
  is both a compliance artifact and a technical safety measure. Reuse the
  same audit infrastructure from
  [02-security-core.md §1](02-security-core.md), don't build a parallel one.

## "pro" design

Offensive-security tool use is its **own elevated capability tier**, layered
on top of the base capability/sandbox system, not a normal `run`/`net`
grant:

1. Explicit scope declaration (target list) required before any scan/exploit
   executes, validated at the tool-adapter layer.
2. Network-layer enforcement of that scope, independent of model judgment.
3. Authorization gate (signed scope doc or verified professional role)
   required specifically to unlock exploitation-class tools; recon-class
   tools can be less gated.
4. Mandatory, immutable audit log of every invocation and finding.
5. Adapters ship for the standard toolchain (Nmap, Nuclei, Burp, Metasploit,
   SQLmap) rather than the agent improvising raw shell commands against
   these tools — every adapter call is one auditable, typed action, not
   free-form shell execution.

This pack should only ship once [02-security-core.md](02-security-core.md)
§1 (capability/sandbox) and its audit-log mechanism exist — it depends on
them directly rather than inventing its own trust model.

## Validated additions from field evidence

Two documented incidents sharpen the design above beyond what the
landscape survey alone implies — see
[11-user-research-pain-points.md §3](11-user-research-pain-points.md) for
full sourcing.

- **Three-state finding schema.** Automated scanners run 30-60%
  false-positive rates; pentesting exists specifically to answer "is this
  actually exploitable here." Some AI-native tools market "zero false
  positives" — treat such claims as requiring independent verification, not
  an established benchmark. Every finding "pro" surfaces carries one of
  three states: `flagged` (scanner-only, unconfirmed), `validated` (exploit
  attempted and succeeded against the declared scope), `blocked` (attempted,
  prevented by a control, auto-downgraded) — turning "reduce false
  positives" into an auditable field rather than a vague product claim.
- **Out-of-band, non-self-attested authorization.** Anthropic disclosed a
  state-sponsored group hijacking Claude Code instances for autonomous
  cyber espionage against ~30 targets, with the AI handling 80-90% of
  tactical operations independently — the attack succeeded specifically by
  having operators **claim legitimate pentest/research authorization
  in-conversation** to bypass safety filters. NIST red-team research
  separately found novel agent-hijacking techniques reaching an 81% success
  rate vs. 11% for the strongest known baseline attack. Given this, the
  authorization gate (item 3 in the "pro" design list above) must require a
  **cryptographically-
  verifiable, time-bounded scope document signed by a role outside the
  agent's own conversation context** (a separate out-of-band approval
  system) — never accept a natural-language authorization claim made within
  the chat itself, since that is precisely the mechanism the documented
  attack exploited.

These two additions apply the same evidence-driven discipline used
throughout [02-security-core.md](02-security-core.md): a control exists
because a specific, named failure mode demonstrated the gap, not because it
sounds prudent in the abstract.

- **Leadership/CISO visibility gap.** Field data: 50% of CISOs cite lack of
  internal AI-security expertise, 48% cite limited visibility into AI usage;
  71% say AI has access to core business systems but only 16% govern that
  access effectively; 92% lack full visibility into AI identities (see
  [11-user-research-pain-points.md §3](11-user-research-pain-points.md)).
  The **capability inventory view** specified in
  [02-security-core.md §1](02-security-core.md) — every active grant, who
  granted it, when it expires, what it's touched — directly serves this
  audience: security leadership gets the same standing, queryable "what can
  every agent/session currently do" view that DevOps uses for its own
  visibility gap, built from data the grant/audit system already collects
  rather than a bespoke leadership dashboard.
