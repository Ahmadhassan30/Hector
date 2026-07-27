# Hector Agent Governance

This file governs all work in the Hector repository.

## Authority and document order

When instructions conflict, use this order after system and explicit user
instructions:

1. the active graph node contract in
   `docs/graph/MASTER_BUILD_GRAPH.md`;
2. `docs/architecture/INVARIANTS.md`;
3. `docs/product/PRODUCT.md` and
   `docs/product/CONVERSATION_PRINCIPLES.md`;
4. `docs/architecture/ARCHITECTURE.md`;
5. the relevant accepted decision record;
6. `docs/governance/EXECUTION_LOOP.md`,
   `docs/governance/BENCHMARKING.md`, and `CONTRIBUTING.md`.

Stop and request direction if two authorities at the same level contradict
each other or if an entry condition cannot be demonstrated.

## Graph discipline

- Work on exactly one execution node at a time.
- Inspect and record predecessor evidence before implementation.
- Do not begin a node whose entry conditions have not passed.
- Treat allowed and forbidden scope as hard boundaries.
- Produce behavioral evidence, not merely successful command exits.
- Stop at the node boundary after its evidence report. Do not begin a
  successor implicitly.
- H02-E is the next node after H01-E.
- H03-E is blocked until the x64 MSVC linker and Windows SDK pass an actual
  native compile/link/run check.
- No real ASR, LLM, or TTS model, runtime, or weight may be downloaded or
  integrated before H36-E declares Gate 0 GO.

## Naming

- Use `Hector` for the human-facing product.
- Use lowercase `hector` for Rust binaries, package/crate prefixes, and
  file-safe identifiers.
- Rust crates use the `hector-` prefix.

## Architecture rules

- Use a functional core and imperative shell.
- The conceptual reducer is
  `reduce(current_state, domain_event) -> transition`.
- A transition contains next state and typed effects, with no direct I/O.
- Infrastructure interprets effects and returns outcomes as domain events.
- Infrastructure and workers never mutate authoritative application state.
- Add a trait only for a demonstrated adapter boundary, test seam, or multiple
  implementation need. Do not build a speculative ports catalogue.
- `hector-runtime` is orchestration-only. It must not own dialogue, memory,
  book, delayed-English, or session-reflection policy.
- Memory, knowledge, features, storage, UI, audio, protocol, and Windows
  process mechanics remain separate ownership boundaries.

## Identity and freshness

Keep these meanings distinct:

- `SessionId`: persistent conversation session identity.
- `TurnId`: session-local turn identity.
- `GenerationEpoch`: process-lifetime assistant generation identity and the
  sole freshness fence for assistant output.
- `RequestId`: tracing and correlation only.
- `AudioEpoch`: capture/playback discontinuity identity.

Never accept assistant output by request ID, turn ID, arrival order, worker
identity, or an unqualified Boolean cancellation flag. Barge-in and other
urgent generation-sensitive controls must carry or atomically publish the
observed `GenerationEpoch`.

## Real-time audio rules

Audio callbacks may perform only:

- fixed, preallocated conversion;
- bounded copy;
- try-push or try-pop on bounded wait-free SPSC queues;
- silence filling;
- atomic counter or epoch operations.

Audio callbacks must not allocate, lock, await, log, resample, infer, access
the filesystem, render terminal output, call the reducer, or mutate central
state. Capture overload discards stale backlog, advances `AudioEpoch`, and
recovers toward recent clean audio.

## Playback truth

Track phrase states explicitly: queued, synthesized, playback started, fully
rendered, partially interrupted, and discarded before playback. Only fully
rendered text may be treated as heard.

## Product and conversation boundaries

- Live philosophical dialogue, thematic memory, delayed English reflection,
  and session reflection are separate systems.
- The Knowledge Engine assembles source-aware, budgeted context before an LLM
  request. The LLM does not own retrieval or memory acceptance.
- Follow `docs/product/CONVERSATION_PRINCIPLES.md` as behavioral governance,
  not as text to paste mechanically into prompts.
- Never turn live philosophical thought into continuous grammar correction.

## Privacy and repository hygiene

Do not commit secrets, model weights, third-party runtime binaries, personal
raw audio, conversation databases, generated speech, private benchmark
fixtures, logs, or machine-specific configuration. Workers may not write
conversation or memory storage directly.

Native Windows execution is the reference mode. Optional containers are
post-native, non-gating worker adapters. They must not create public endpoints
or become a required dependency.

## Required execution behavior

Follow the complete inspect-to-evidence loop in
`docs/governance/EXECUTION_LOOP.md`. For risky nodes, run its adversarial
failure-injection sub-loop. Inspect the complete diff and compare it with these
invariants before declaring a node passed.
