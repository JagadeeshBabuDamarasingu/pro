# pro — open-source maintainers

Distinct from [03-audience-developers-builders.md](03-audience-developers-builders.md):
this audience's job is judging *other people's* (increasingly AI-generated)
submissions at scale under severe time pressure, not generating their own
code. The cleanest reuse story of any newly-identified audience — it's the
Developer pack's own machinery, pointed at the reviewer's chair instead of
the author's.

## Landscape and evidence

No dedicated tooling exists for this today — maintainers use generic
GitHub tooling plus ad hoc triage bots.

- **curl shut down its bug-bounty program** because AI-generated
  vulnerability reports were wasting maintainer time (already cited in
  [11-user-research-pain-points.md §1](11-user-research-pain-points.md));
  Daniel Stenberg has written and spoken repeatedly about "AI slop" reports
  specifically, and it's a recurring theme at maintainer-focused
  conference tracks (FOSDEM and similar).
- **The XZ Utils backdoor** (CVE-2024-3094, discovered March 2024) is the
  landmark supply-chain incident enabled specifically by a lone, overloaded
  maintainer accepting a "helpful" long-term contributor's changes — now
  the standard citation for "maintainer bandwidth is a security boundary,
  not just a productivity problem," directly motivating a contributor-trust/
  provenance layer, not only a code-quality one.
- Recurring OSS-sustainability survey findings (Linux Foundation/Tidelift-
  style research) consistently identify maintainer burnout and
  under-resourced triage load as the top structural risk to the supply
  chain — AI-generated low-effort PRs/issues are widely reported to be
  *increasing* triage load, since volume is rising faster than quality.

## "pro" design — reuse, not new architecture

1. **Orthogonal review and the AI Slop Index map almost unchanged** onto
   "score this incoming external PR/issue for AI-slop probability before a
   human maintainer spends time on it" — the exact same mechanisms from
   [02-security-core.md §4](02-security-core.md), applied to inbound
   contributions instead of the user's own agent output.
2. **Confidence signals/resampling on inbound bug reports** distinguish a
   real, reproducible issue from a plausible-sounding but non-reproducible
   hallucinated one (the curl failure mode) — a triage-time filter, using
   the identical SelfCheckGPT-style mechanism already specced, rather than
   a code-generation-time one.
3. **A new contributor-trust tier**, separate from the internal autonomy
   tiers, scopes how much unattended verification (auto-run the PR's tests
   in an isolated worktree, auto-run a security scan) a maintainer's agent
   performs on an *external, untrusted* submission before a human ever
   looks at it. This reuses the existing worktree-isolation infrastructure
   ([03-audience-developers-builders.md](03-audience-developers-builders.md))
   as the execution sandbox for untrusted code — a low-effort extension of
   infra already planned, not new architecture.
4. **graphify surfaces prior triage decisions**: a project's own issue/PR
   history ("we rejected this pattern before, here's why") is exactly a
   graphify corpus — surfacing that context during triage directly reuses
   the knowledge-graph mechanism from
   [10-cost-optimization.md §5](10-cost-optimization.md).

## Net-new work

A GitHub/GitLab/Gerrit-native triage surface (label suggestions,
duplicate-detection against issue history, auto-drafted "here's what to
test" comments) and the contributor-trust tier described above — UI/
connector work and one new trust dimension, not new core architecture.
