# Hector Architecture Invariants

These are pass/fail constraints, not preferences.

## State ownership

1. The main Rust process owns authoritative application state.
2. Domain decisions occur only through the pure reducer.
3. A transition contains next state and typed effects, with no direct I/O.
4. Infrastructure performs effects and returns outcomes as events.
5. Workers, storage, audio, UI, and model adapters never mutate central state.

## Identity and freshness

1. `SessionId` identifies a session.
2. `TurnId` is local to a session.
3. `GenerationEpoch` is process-lifetime and is the sole assistant-output
   freshness fence.
4. `RequestId` is tracing-only.
5. `AudioEpoch` identifies discontinuities, not assistant freshness.
6. Every generation-sensitive request, result, cancellation, and urgent
   barge-in observation is qualified by `GenerationEpoch`.
7. Arrival order, request ID, turn ID, worker identity, or a Boolean flag cannot
   make assistant output fresh.

## Real-time audio

Audio callbacks may only perform fixed/preallocated conversion, bounded copy,
try-push/try-pop, silence filling, and atomic counter/epoch operations.

Callbacks must never:

- allocate or free heap memory;
- acquire a lock or wait;
- enter async code;
- log or render terminal output;
- resample;
- perform inference;
- access files or databases;
- call the reducer or mutate application state.

Resampling, inference, logging, recovery policy, and state transitions occur
off-callback.

## Overload and recovery

1. Audio boundaries are bounded wait-free SPSC queues.
2. A capture overrun is a discontinuity.
3. Recovery advances `AudioEpoch`, resets dependent consumers, discards stale
   backlog, and moves toward recent clean audio.
4. Old microphone audio is never preserved indefinitely for completeness.
5. Callback work and urgent control have declared bounded cost.

## Playback truth

A phrase has explicit states:

- queued;
- synthesized;
- playback started;
- fully rendered;
- partially interrupted;
- discarded before playback.

Only fully rendered text may be assumed heard. Synthesis completion, queueing,
or playback start is insufficient. Playback reports precise rendered progress
to the non-real-time ledger owner.

## Workers and processes

1. ASR, LLM, and TTS are replaceable adapters.
2. Worker protocol messages are bounded, versioned, and model-neutral.
3. stdout is protocol-only; diagnostics use stderr.
4. Worker crashes, malformed output, hangs, and restarts become typed events.
5. Every worker process tree belongs to a Windows Job Object.
6. No supervised descendant may survive main-process shutdown.
7. Restarts are bounded; restart loops cannot run indefinitely.

## Memory, knowledge, and product behavior

1. The Memory Engine proposes and evaluates memories but performs no I/O.
2. Storage persists accepted decisions but cannot decide importance,
   duplication, conflict, consolidation, or retrieval relevance.
3. Memories retain provenance; changing beliefs form a timeline.
4. Conflicts are explicit and cannot be silently overwritten.
5. Books are distinct from Ahmad's autobiographical and intellectual memory.
6. The Knowledge Engine assembles traceable context within an explicit budget.
7. The LLM does not own retrieval planning, context budgeting, or memory
   acceptance.
8. Dialogue, delayed English reflection, and session reflection remain separate
   feature modules.
9. `hector-runtime` contains orchestration, not product behavior.

## Conversation

Behavior must be evaluated against
`../product/CONVERSATION_PRINCIPLES.md`. Live philosophical thought cannot be
repeatedly interrupted by grammar correction. Conversation Principles are
model-independent criteria rather than prompt text.

## Privacy and deployment

1. Personal conversations, raw audio, databases, generated speech, model
   weights, and private fixtures are not committed.
2. Workers cannot write application storage directly.
3. Native Windows mode is the reference deployment.
4. Containers are optional, post-native, and non-gating.
5. Optional container endpoints are private/loopback by default.
6. No product or domain layer depends on Docker or public HTTP.

## Gate invariants

1. No real model runtime or weight is introduced before H36-E Gate 0 GO.
2. No Cargo workspace is created before H02-E proves native MSVC linking.
3. No graph node begins before every predecessor and entry condition passes.
4. A command's successful exit is not sufficient evidence of behavioral pass.
