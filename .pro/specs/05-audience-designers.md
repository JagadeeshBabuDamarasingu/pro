# pro — designers

## Landscape

Figma's own AI has consolidated: **First Draft** (text → editable native
layout) was folded into a broader **Figma Agent** (May 2026) adding
re-prompting/bulk edits/live feedback across flows; **Figma Make** turns
prompts into interactive prototypes with real logic embedded; **AI Grid**
(realistic placeholder content) is reportedly the most-used day-to-day AI
feature, ahead of generation itself.

Third-party generators: **Google Stitch** (formerly Galileo AI, acquired
mid-2025) — prompt/sketch → hi-fi wireframe with Figma + HTML/Tailwind
export; **Uizard** — text-to-UI and a genuinely distinct input modality
(photograph a paper sketch → editable digital design), best for fast
concepting; **v0** (Vercel) — code-first, generates working React/Next.js
directly; **Locofy** — Figma→React/Next.js/Flutter/React Native with
AI auto-tagging; **TeleportHQ** — cheaper, dependency-free multi-framework
output.

**Builder.io Visual Copilot** is the standout for fidelity: it maps
generated code to your *existing* component library rather than generating
parallel/duplicate components, and ships a CLI for scriptable Figma→code
conversion. This — not raw generation quality, which is a crowded, largely
commoditized space — is the differentiator "pro" should compete on.

## Figma integration depth

Three integration surfaces, matched to depth needed:

1. **Plugin API** — runs inside the editor, full read+write on the scene
   graph; needed only if "pro" ships an in-canvas panel.
2. **REST API** — headless sync: files, library sync, and (2026) comment
   endpoints (`as_md` markdown export), annotations, dev-resource links.
3. **Dev Mode MCP server** — exposes structured hierarchy, layout/auto-layout
   rules, text styles, component properties, variables/tokens directly to
   any MCP client (Claude Code, Cursor, Windsurf, Copilot). Includes
   **Code Connect** (maps components to real code paths, so generated code
   references the actual library) and **write-to-canvas** (agents can
   create/modify native Figma content using the design system as source of
   truth).

**"pro" design:** architect around the **MCP server** for agentic read/write
(best token efficiency, Code Connect fidelity) plus the **REST API** for
headless sync jobs (comments/annotations/library sync). Only build a Plugin
API panel if an in-canvas UI becomes a real requirement.

## Validated pain points and required guardrails

Real Figma-community complaints sharpen the design above — see
[11-user-research-pain-points.md §5](11-user-research-pain-points.md) for
full sourcing.

- **Reference-grounded generation is mandatory, not optional.** A
  documented case: a user attached 8 reference images with explicit style
  instructions and Figma Make ignored them entirely, producing a generic
  app — prompt-only generation forces the model to guess spacing,
  typography, and color from nothing. "pro" always pulls the live Dev Mode
  MCP structured hierarchy (auto-layout rules, text styles, variables) as
  generation context before any prompt is sent; a user cannot skip to
  blank-prompt generation without a connected file.
- **Deterministic component/token grounding gate** — the design analog of
  the AST/LSP symbol-grounding gate in
  [02-security-core.md §4](02-security-core.md). A documented case found
  27 of 27 design-token hallucinations in an early agent version, dropping
  to zero once structured rules were introduced; a subtler failure is a
  token that exists structurally but is used **outside its documented
  semantic contract** (a primitive token used where a semantic token was
  intended) — invisible to a model that only sees the token name. Every
  generated reference to a component ID or token name must resolve against
  the real Code Connect mapping and variable set pulled live via MCP;
  contract-violating usage is flagged, not silently allowed.
- **Scope-discipline diffing on canvas writes.** Documented complaint:
  "when I ask for a specific change, Figma decides to change other sections
  that had nothing to do with my request," sometimes affecting logic, with
  users stuck in a repeated failure loop escapable only by reverting to an
  earlier save. Apply the same scope-discipline axis already in the AI Slop
  Index ([02-security-core.md §4](02-security-core.md)) to design diffs:
  before writing back to canvas via MCP, diff the proposed change against
  the current file tree and flag anything outside the stated target as a
  separately-approvable change, never auto-bundled into the primary edit.
- **Pre-flight cost/confidence estimate before spending credits.** A
  documented case: a generation attempt failed to implement a single
  change while consuming a user's entire credit balance. Surface the
  confidence-gating logic from
  [02-security-core.md §4](02-security-core.md) as a pre-flight check — low
  confidence of matching the design system (per the grounding gate above)
  warns before compute/credits are spent, not after a failed attempt.
- **Provenance ("why") trail on generated output**, the same
  elevation-prompt provenance principle already specified for tool-use in
  [02-security-core.md §2](02-security-core.md), applied to generation:
  which reference screens/tokens/patterns informed this output, surfaced
  alongside the result — this is the difference between a designer
  steering an accelerant and rubber-stamping a black box, which community
  sentiment identifies as the actual line between augmentation and
  replacement anxiety.

## Mobbin-like reference library (later phase)

Mobbin itself: 400,000+ searchable screenshots across ~1,000+ apps, search
by Screens/UI Elements/Flows/OCR text, full flow visualization (not just
static frames), Figma plugin to copy discovered designs directly in.

Build approach if pursued:

1. **Ingestion** — automated screenshot capture (app-store scraping or a
   scripted device-farm/emulator crawl).
2. **Structural extraction** — VLM (or a trained object detector) outputs
   bounding boxes/component types per screenshot → powers "UI Elements"
   search.
3. **Semantic embeddings** — CLIP for an MVP; note the caveat from recent
   research (UISearch, arXiv 2511.19380) that CLIP generalizes on semantic
   *category* but struggles at *instance discrimination* (two structurally
   different but semantically similar screens) — UISearch's graph-based
   representation (element relationships + spatial layout via a contrastive
   graph autoencoder) is the reference architecture for best-in-class
   retrieval, not just an MVP CLIP index.
4. **Tagging taxonomy** — controlled vocabulary (onboarding, empty state,
   paywall, checkout, settings...) via VLM auto-tagging + manual curation.
5. **Flow reconstruction** — capture crawler click-graphs as ordered
   screenshot sequences with transition metadata.
6. **Hybrid search** — OCR (text-in-screenshot) indexed into full-text
   search, combined with the vector layer for simultaneous semantic +
   visual + full-text + metadata queries.
7. **Legal note** — scraping third-party app screenshots for a commercial
   reference library raises IP/ToS questions Mobbin has presumably solved
   through scale/negotiation; needs legal review before commitment. An
   alternative seed strategy: build from the user's own design system plus
   explicitly licensed/public sources first, expand later.

This is explicitly a **later-phase** feature (see
[09-roadmap.md](09-roadmap.md)) — the MCP/Code-Connect integration and
design-to-code fidelity are the v1 differentiator; the reference library is
a v2+ bet with real legal diligence required first.
