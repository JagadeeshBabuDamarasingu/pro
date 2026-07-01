# pro — cloud task execution

A fourth execution target for the shared core engine — alongside local,
git-worktree-parallel, and CI — that lets a task run unattended on
"pro"-managed (or self-hosted) cloud compute rather than the user's own
machine, matching the capability now standard across Claude Code (cloud/
background sessions), Cursor (Background Agents / "Cursor Cloud"), GitHub
Copilot's coding agent, and Devin.

**Core principle: cloud is a fourth execution *target*, not a fourth,
separately-trusted surface.** Per
[01-architecture.md](01-architecture.md), GUI/CLI-TUI/CI are all thin
clients of one Rust core that owns orchestration, the capability/sandbox
system, and the audit log. Cloud execution reuses that exact substrate —
the policy layer, autonomy tiers, blast-radius bounds, and audit schema
from [02-security-core.md §1](02-security-core.md) do not fork for cloud;
only the **enforcement venue** changes. Every documented incident in the
field (below) traces back to a vendor treating cloud as a place where
convenience won over consistent enforcement — "pro" should not repeat that.

## What competitors actually do

- **Claude Code cloud/background sessions**: triggered via the web
  dashboard, `claude --remote "<task>"` from the CLI, `/schedule` (cron-style
  Routines), or `@Claude` in Slack. Each session runs in a fresh
  Anthropic-managed VM with the repo cloned; the same OS-level sandbox rules
  (bubblewrap/Seatbelt) and capability policy apply as local — cloud only
  swaps *where* enforcement happens. Git credentials never enter the
  sandbox: a dedicated GitHub proxy translates a scoped in-sandbox
  credential to the real token outside it, and push is restricted to the
  session's own branch. Documented gap: no dedicated secrets store yet for
  cloud env vars (visible in plaintext to anyone who can edit the
  environment) — a gap "pro" should not inherit.
- **Cursor Background Agents / "Cursor Cloud"**: triggered from the web
  dashboard, desktop app, Slack (`@cursor`), GitHub/Linear mentions, or the
  iOS app. Each agent gets its own isolated VM mirroring a full dev
  environment; parallelism is explicitly unlimited. Secrets are scoped at
  workspace/team level via a dedicated Secrets tab, with STS
  role-assumption support (`CURSOR_AWS_ASSUME_ROLE_ARN`) and TOTP storage —
  i.e., short-lived credentials over static keys wherever possible. Results
  land as merge-ready PRs with verification artifacts (screenshots, logs).
- **GitHub Copilot's coding agent**: triggered by assigning an issue to
  Copilot or `@copilot` in a PR comment. Runs as an ephemeral GitHub
  Actions environment with a default-on firewall and a maintained
  allowlist. **Strongest guarantee among all four**: Copilot cannot approve
  or merge its own PRs and can only push to `copilot/*` branches — standard
  branch protection and CODEOWNERS govern it exactly as a human
  contributor. It also gets its own dedicated "Agents" secrets namespace,
  distinct from general CI secrets, specifically to avoid handing a
  semi-autonomous agent the same blast radius as a deploy pipeline.
- **Devin**: fully cloud-native — each session is a disposable VM with its
  own shell/editor/headless-browser, default-deny egress with an allowlist,
  destroyed at session end. Multiple sessions run concurrently, unified in
  a Progress-tab dashboard a human can scrub/interrupt/take over. A
  documented April 2025 incident (Embrace The Red) showed prompt injection
  from an untrusted GitHub issue causing Devin to fetch and execute
  attacker-controlled code, exposing AWS keys present in the sandbox — one
  of the earliest concrete "lethal trifecta" exploits against a commercial
  cloud coding agent.

**Common pattern across all four**: ephemeral compute per session; repo
access via a scoped-token-proxy, never a raw SSH key/PAT in the sandbox;
secrets in a narrow, named store distinct from general CI secrets; trigger
surfaces converging on CLI command / web dashboard / chat mention / GitHub
webhook / scheduled cron; results converging on draft PR + diff viewer +
notification, never a direct push to a shared branch; a default-on egress
allowlist with an explicit escape hatch. **None of the four vendors have
shipped a general fix for prompt-injection-into-privileged-action** — every
documented incident (Devin's AWS-key leak, a 2026 Claude Code Action
comment-hijacking pattern stealing API keys/tokens, GitHub Copilot's
CVE-2026-41109 consent-prompt bypass, Gemini CLI's CVSS 10.0 headless-mode
RCE) is a variant of the same failure, and current mitigations are
allowlist/scoping-based damage limitation, not injection prevention.

## "pro" design

1. **Cloud is selected the same way CI already is**: `pro run --target
   cloud` alongside implicit `local`/`worktree`/`ci`. The core engine,
   policy layer, and audit schema do not fork per target.
2. **The cloud VM is the OS-enforcement layer's remote instance, not a new
   trust tier.** The same capability grammar (`read`/`write`/`run`/`net`/
   `env`, argv-validated, scoped) governs cloud execution; a task's
   capability grant object is **serializable and portable** — the same
   grant that governs a local worktree run governs a cloud run, shipped to
   a different enforcement point. Use a real OS/microVM sandbox in the
   remote environment (bubblewrap/Landlock, or a Firecracker-class microVM
   — the documented category standard, used by E2B and others, for
   kernel-per-sandbox isolation and fast snapshot-based boot) rather than a
   bare container with no kernel isolation.
3. **Git-worktree parallel execution is the cloud scheduling unit.** The
   best-of-N pattern already specified in
   [03-audience-developers-builders.md](03-audience-developers-builders.md)
   (`pro run --parallel N --verify <test-cmd>`) extends directly to
   `pro run --target cloud --parallel N`: each worktree becomes a cloud
   session, and the same verifier gate decides which result surfaces for
   review. Local-worktree isolation and cloud-VM isolation are two
   instances of one idea — don't build a separate cloud-parallelism concept
   or a separate scheduler/review UI.
4. **Credential-broker proxy from day one**, not Claude Code's documented
   stopgap (plaintext env vars in the cloud environment config). The cloud
   sandbox never holds a raw git PAT, cloud-provider key, or signing key —
   it holds a short-lived, task-scoped credential that a broker outside the
   sandbox translates and audits per request, mirroring Claude Code's
   GitHub-proxy pattern and Cursor's STS-role-assumption support. This is a
   direct extension of [02-security-core.md §1](02-security-core.md)'s
   `env: scoped to named vars only` rule — for cloud, "named vars" means
   broker-issued, TTL-bound tokens, because there's no human present to
   re-auth interactively.
5. **No new, looser autonomy defaults for cloud** — if anything, cloud
   execution runs unattended by definition (the "close your laptop" pitch
   every vendor makes), so there's no operator present to notice something
   going wrong in real time the way there is locally. The autonomy tiers
   and blast-radius bounds from
   [02-security-core.md §1](02-security-core.md) apply with no cloud
   carve-out, and any cloud task touching a shared repo inherits Copilot's
   strongest guarantee as a hard floor: **no self-merge, restricted branch
   namespace (`pro/*`), full branch-protection/CODEOWNERS inheritance**,
   regardless of autonomy tier.
6. **Injection defenses are mandatory, not optional, for the cloud
   target.** Every documented cross-vendor incident is a lethal-trifecta
   variant, and cloud is structurally the worst case for it — independent
   network egress, often unattended for long stretches. Any cloud session
   ingesting untrusted content (issues, PR comments, web fetches) must go
   through the quarantine pattern and CaMeL-style taint tracking from
   [02-security-core.md §2](02-security-core.md) before that content can
   influence a privileged action, and MELON-style masked re-execution is
   required before any cloud session's `git push`, a run against a
   production-shaped target, or a credential-scope escalation.
7. **Audit trail: session-transcript trailer, same causal classification.**
   Every cloud-produced commit/PR carries a `pro-session:` trailer linking
   back to the full evidence chain in the audit log, reusing the existing
   `reasoning_summary`/`evidence_chain`/hallucination-injection-privilege-
   drift classification fields rather than inventing cloud-specific
   telemetry. The capability-inventory view
   ([02-security-core.md](02-security-core.md)) shows cloud grants
   (with broker-issued credential TTLs) in the same inventory as local and
   CI grants — "what can our agents currently do" is one query regardless
   of execution target.
8. **Trigger surfaces**, matching the field-standard set: CLI
   (`pro run --target cloud`), GUI ("run in cloud" action on any task),
   webhook/chat mention (issue-assign, PR-comment `@pro`, Slack), and
   scheduled/cron (reuses the DevOps pack's CI-safe dual-mode design,
   [06-audience-devops.md](06-audience-devops.md)) — a cloud task is, from
   the core engine's point of view, just a CI-mode invocation with a
   different compute venue and a broker-issued credential set.
9. **Self-hosted cloud pool as an escape hatch**: for teams that don't want
   task compute running on "pro"-managed infrastructure, support pointing
   the cloud target at self-hosted runners (mirroring Cursor's
   "Self-Hosted Pool" and "My Machines"), reusing the same broker/sandbox
   design rather than a separate self-hosted code path.

The net shape: cloud execution in "pro" should look, from the security
model's point of view, like **local execution running on hardware you
don't own** — same capability policy, same sandbox enforcement, same audit
schema, same autonomy ceilings — with exactly three things that genuinely
differ because a human isn't physically present: a credential broker
instead of live interactive auth, mandatory quarantine/injection defenses
given longer unattended run windows, and a portable capability-grant object
so the same policy document governs whichever venue actually executes the
task.
