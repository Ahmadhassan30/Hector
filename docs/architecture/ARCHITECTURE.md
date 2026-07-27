# Hector Architecture

## Baseline

Hector is a Windows-first native Rust application. It has no browser UI,
Electron, Tauri, JavaScript frontend, FastAPI frontend, or public HTTP service.
The reference acoustic mode is headphones-first.

Rust owns audio, timing, orchestration, domain state, cancellation, playback
acceptance, persistence policy, worker supervision, and Ratatui UI. CPAL/WASAPI
provides microphone and playback access. Tokio is restricted to the
non-real-time control plane. SQLite is the local persistence layer. Windows Job
Objects own supervised process trees.

## Functional core, imperative shell

The pure core makes decisions:

```text
reduce(current_state, domain_event)
  → transition { next_state, typed_effects }
```

The reducer performs no I/O. Infrastructure interprets typed effects and
returns results as domain events. Infrastructure, adapters, workers, and UI
never mutate authoritative state directly.

Clocks, metrics, audio, ASR, LLM, TTS, supervision, and persistence are
conceptual ports. A Rust trait is introduced only when a real adapter boundary,
test seam, or multiple implementation requires it.

## Ownership layers

- `hector-core`: identifiers, state, events, effects, reducer, playback ledger.
- `hector-protocol`: worker framing and transport-neutral messages.
- `hector-audio`: callback-safe queues, CPAL streams, resampling, recovery.
- `hector-platform-windows`: Job Objects and Windows process mechanics.
- `hector-storage`: SQLite, records, transactions, and migrations.
- `hector-runtime`: coordinator, effect runner, worker clients, clocks, metrics,
  and adapter wiring only.
- `hector-memory`: deterministic memory-quality policies.
- `hector-knowledge`: memory/book retrieval planning and bounded context.
- `hector-features`: dialogue, delayed English reflection, session reflection.
- `hector-tui`: immutable projections and typed user intentions.
- `apps/hector`: composition root.

Memory, knowledge, and features are created only at H47-E, H49-E, and H50-E.
Their planned ownership does not justify empty crates during bootstrap.

## Control and data flow

```text
external observation
  → domain event
  → reducer
  → transition
  → typed effects
  → runtime adapters/workers
  → result events
```

The main process owns all state and acceptance decisions. Workers are
replaceable crash boundaries. They communicate through bounded, versioned,
framed local IPC. Protocol stdout contains messages only; diagnostics use
stderr. A worker cannot persist conversation or memory state.

The initial IPC assumption is redirected stdin/stdout. Named pipes or another
local transport require measurement and a decision; domain behavior cannot
depend on the choice.

## Runtime topology

```text
Microphone → CPAL/WASAPI → bounded audio queues
                              ↓
                       main Rust process
                  coordinator → reducer → effects
                    ↓          ↓          ↓
              ASR worker   LLM worker   TTS worker
                    ↓
             CPAL/WASAPI → headphones
```

The main process also owns Ratatui, SQLite policy, the phrase ledger, memory and
knowledge policies, and the Windows Job Object supervisor.

## Model replacement and Gate 0

Fake ASR, LLM, and TTS workers must prove coordinator flow, cancellation,
stale-output rejection, phrase reconciliation, fault behavior, process cleanup,
and soak stability before any real model work. llama.cpp/llama-server is the
intended replaceable LLM runtime family, not a domain dependency.

Model-specific prompts and response types remain in adapters. The Knowledge
Engine supplies a source-aware, budgeted context before an LLM request.

## Deployment profiles

Native Windows workers are the reference and required mode. After native
integration and regression benchmarks pass, an optional container adapter may
launch or connect to local ASR/LLM/TTS containers. It must preserve the same
protocol, `GenerationEpoch`, cancellation, ledger, and supervision semantics.

Container support is non-gating, exposes no public endpoint by default, stores
weights outside Git, and cannot enter core, memory, knowledge, or feature
dependencies.

## Deferred scope

Public distribution, signed installers, accounts, cloud sync, multi-user
support, public APIs, commercial qualification, and speaker-mode AEC are not
part of the current build graph.
