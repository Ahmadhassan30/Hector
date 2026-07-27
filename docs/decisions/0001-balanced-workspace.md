# ADR 0001: Balanced Product-Aware Workspace

- Status: Accepted for future implementation after H02-E
- Date: 2026-07-28

## Context

Hector needs strong boundaries around deterministic state, real-time audio,
replaceable workers, Windows process cleanup, storage, memory, knowledge, and
conversation behavior. It is also maintained primarily by one developer.
Maximizing crate count would turn immature policies into public APIs, while a
few broad crates would make safety depend on convention.

## Options

### A: Broad and speed-oriented

Use an app plus broad core and platform crates. This has low bootstrap and
compile overhead, but orchestration and platform crates are likely to become
dumping grounds. Audio safety and worker replacement remain weakly enforced.

### B: Balanced and product-aware

Use app, core, protocol, audio, runtime, Windows platform, storage, and TUI
boundaries initially. Create memory, knowledge, and feature crates only at
their implementation nodes.

This provides clear dependency direction and test seams with moderate
maintenance cost.

### C: Maximum isolation

Give identifiers, reducer, effects, audio layers, supervision, each memory
stage, books, context construction, dialogue, English reflection, storage, and
test utilities separate crates.

This provides maximum mechanical isolation but excessive manifests, APIs,
navigation, and compile orchestration for a one-person system.

## Decision

Choose Option B.

The mature target contains:

- `hector-core`;
- `hector-protocol`;
- `hector-audio`;
- `hector-runtime`;
- `hector-platform-windows`;
- `hector-storage`;
- `hector-tui`;
- later `hector-memory`, `hector-knowledge`, and `hector-features`;
- `apps/hector`.

Books begin as an isolated subsystem inside `hector-knowledge`.

## Adversarial corrections

- Do not create a generic ports crate. Traits live with their consumer and only
  appear for demonstrated boundaries.
- Keep identifiers, state, events, effects, reducer, and phrase ledger as
  modules in core rather than separate crates.
- Keep `hector-runtime` orchestration-only.
- Keep CPAL/audio mechanics outside the Windows process crate.
- Keep SQL outside memory and knowledge policies.
- Keep model prompts outside core and product-domain types.
- Do not create memory, knowledge, features, books, benchmarks, workers, or
  deployment placeholders before their graph nodes.

## Consequences

The repository gains enforceable high-risk boundaries without maximum
granularity. Runtime ownership needs continuous review. Product crates add
compile units later, when their distinct behavior justifies the cost.

## Revisit conditions

Revisit this decision only when evidence shows:

- a module needs independent reuse or release;
- compile times or dependency churn require a split;
- an existing crate repeatedly violates ownership despite enforcement;
- book ingestion/indexing becomes independently substantial;
- two crates remain inseparable and a merge would simplify invariants.

A revision requires a new superseding ADR rather than editing this decision's
history.
