# Hector Master Build Graph

No node may begin before every predecessor and entry condition passes. Every
execution node follows `../governance/EXECUTION_LOOP.md` and stops at its own
boundary.

## Overview

```text
H00-P → H01-E → H02-E ─pass→ H03-E → H04-E
                    └fail→ H02-R → H02-E

H04 → system spine + native audio
spine/audio join → H30 → H31 → {H32,H33,H34} → H35 → H36 Gate 0
                                                   ↑       │
                                                   └ H35-R─┘

H36 GO → H37 benchmark foundation → real component branches
real components → memory + books → knowledge → features/integration
H50 integration → H51 mandatory benchmark → H53 personal acceptance
                                      └→ H52 optional containers
```

## Foundation

### H00-P — Repository Reconnaissance and Foundation Planning

- Purpose: establish repository/environment facts and a decision-complete plan.
- Predecessors: none.
- Entry: the existing Hector clone is readable.
- Allowed scope: read-only repository and environment inspection.
- Forbidden scope: mutation, installation, download, or implementation.
- Deliverables: accepted H00 planning report.
- Automated validation: Git/file/tool probes and plan consistency review.
- Manual validation: Ahmad reviews product and architecture alignment.
- Pass criteria: report accepted and no repository mutation occurred.
- Failure edges: inaccessible or contradictory repository → human decision.
- Successors: H01-E.

### H01-E — Repository Governance and Architecture Foundation

- Purpose: encode product, conversation, architecture, memory/knowledge,
  benchmark, graph, and workflow governance.
- Predecessors: H00-P.
- Entry: H00 is accepted and the baseline is reconciled.
- Allowed scope: the exact documentation allowlist in H01's approved contract.
- Forbidden scope: Cargo, Rust, installations, models, containers, or code.
- Deliverables: fourteen governing documents.
- Automated validation: allowlist, content, link, whitespace, and contradiction
  checks.
- Manual validation: product and architecture consistency review.
- Pass criteria: documents agree, required contents exist, no implementation.
- Failure edges: repair within H01; unexpected external changes → stop.
- Successors: H02-E.

### H02-E — Environment Verification

- Purpose: prove x64 MSVC native compilation.
- Predecessors: H01-E.
- Entry: governance documents pass.
- Allowed scope: read-only probes and disposable out-of-tree native link test.
- Forbidden scope: unapproved installation or workspace creation.
- Deliverables: environment evidence report.
- Automated validation: tool versions, linker/SDK discovery, compile/link/run.
- Manual validation: reproduce from a documented clean shell.
- Pass criteria: an x64 MSVC executable builds and runs.
- Failure edges: H02-R.
- Successors: H03-E.

### H02-R — Environment Repair

- Purpose: repair only the prerequisites failed by H02-E.
- Predecessors: failed H02-E and explicit human authorization.
- Entry: exact missing components are recorded.
- Allowed scope: approved Build Tools/SDK installation and MSVC selection.
- Forbidden scope: unrelated workloads, models, Docker, or project code.
- Deliverables: repair and revalidation evidence.
- Automated validation: repeat all H02 checks.
- Manual validation: installation scope and disk review.
- Pass criteria: H02 passes without accidental shell state.
- Failure edges: blocked environment branch.
- Successors: H02-E.

### H03-E — Cargo Workspace Bootstrap

- Purpose: create only the immediately required balanced workspace boundaries.
- Predecessors: H02-E.
- Entry: clean tree and verified MSVC linker.
- Allowed scope: core, protocol, audio, runtime, Windows platform, storage, TUI,
  app skeletons, and repository-local toolchain policy.
- Forbidden scope: product features, memory, knowledge, benchmarks, models, or
  container implementation.
- Deliverables: minimal compiling workspace.
- Automated validation: metadata, format, Clippy, tests, dependency inspection.
- Manual validation: naming and boundary review.
- Pass criteria: clean MSVC build without speculative APIs.
- Failure edges: reduce H03 structure or return to H02-R.
- Successors: H04-E.

### H04-E — Architecture-Boundary Verification

- Purpose: enforce dependency direction and platform isolation.
- Predecessors: H03-E.
- Entry: workspace validation is green.
- Allowed scope: boundary tests, dependency rules, lint policy.
- Forbidden scope: product behavior.
- Deliverables: enforceable architecture checks.
- Automated validation: forbidden-dependency and workspace checks.
- Manual validation: cycle and premature-abstraction review.
- Pass criteria: repository layout and dependency invariants hold.
- Failure edges: repair H03.
- Successors: H10-E, H13-E, H14-E, H16-E in parallel.

## System spine

### H10-E — Domain Identifiers

- Purpose: define `SessionId`, `TurnId`, `GenerationEpoch`, `RequestId`, and
  `AudioEpoch`.
- Predecessors: H04-E.
- Entry: core isolation passes.
- Allowed scope: pure types and tests.
- Forbidden scope: I/O or conflated freshness semantics.
- Deliverables: typed identifiers with documented lifetimes.
- Automated validation: construction, ordering, and property tests.
- Manual validation: misuse review.
- Pass criteria: only `GenerationEpoch` can fence assistant output.
- Failure edges: revise H10.
- Successors: H11-E and H13-E.

### H11-E — Domain State, Events, and Effects

- Purpose: establish deterministic state and typed effect vocabulary.
- Predecessors: H10-E.
- Entry: identifier tests pass.
- Allowed scope: pure domain types and invariants.
- Forbidden scope: effect execution, prompts, SQL, or infrastructure handles.
- Deliverables: minimal state/event/effect model.
- Automated validation: invariant and invalid-state tests.
- Manual validation: event/effect completeness review.
- Pass criteria: effects leave core and outcomes return as events.
- Failure edges: repair H11.
- Successors: H12-E.

### H12-E — Pure Reducer

- Purpose: implement `reduce(state, event) -> transition`.
- Predecessors: H11-E.
- Entry: domain vocabulary is accepted.
- Allowed scope: deterministic transition logic and tests.
- Forbidden scope: I/O, async, locks, direct clocks, randomness, mutation by
  infrastructure.
- Deliverables: reducer and transition matrix.
- Automated validation: replay, stale, cancellation, fault, shutdown tests.
- Manual validation: transition-table review.
- Pass criteria: equal inputs produce equal state/effects.
- Failure edges: vocabulary defect → H11; implementation defect → H12 repair.
- Successors: H30-E.

### H13-E — Worker Protocol

- Purpose: bounded, versioned IPC with generation-qualified work and control.
- Predecessors: H04-E and H10-E.
- Entry: identifier semantics are stable.
- Allowed scope: DTOs, framing, versioning, compatibility tests.
- Forbidden scope: model-specific core types, unbounded frames, stdout logs.
- Deliverables: protocol crate and wire contract.
- Automated validation: round-trip, malformed, oversized, truncated, stale,
  version-mismatch tests.
- Manual validation: replaceability and crash-boundary review.
- Pass criteria: lifecycle and freshness semantics are explicit.
- Failure edges: redesign H13.
- Successors: H15-E, H16-E, H30-E.

### H14-E — Audio Primitives

- Purpose: bounded SPSC buffers, preallocation, counters, and urgent epoch
  control.
- Predecessors: H04-E and H10-E.
- Entry: audio crate boundary is isolated.
- Allowed scope: fixed callback-safe primitives.
- Forbidden scope: callback allocation, locks, async, logging, resampling, I/O,
  or reducer calls.
- Deliverables: RT primitives and callback contract.
- Automated validation: capacity, wraparound, concurrency, overrun, silence,
  urgent-control tests.
- Manual validation: callback code audit.
- Pass criteria: steady-state callback work is bounded and allocation-free.
- Failure edges: repair H14.
- Successors: H20-E and H30-E.

### H15-E — Fake Workers

- Purpose: deterministic fake ASR, LLM, and TTS with fault controls.
- Predecessors: H13-E.
- Entry: protocol tests pass.
- Allowed scope: scripted results, delays, reordering, malformed output, hangs,
  crashes, cancellation.
- Forbidden scope: real models, APIs, downloads, or bypassed IPC.
- Deliverables: fake workers and scenario controls.
- Automated validation: all declared lifecycle/fault scenarios.
- Manual validation: verify realistic protocol use.
- Pass criteria: worker conditions are reproducible.
- Failure edges: improve H15 controls or H13 protocol.
- Successors: H30-E.

### H16-E — Process Supervision

- Purpose: launch, monitor, restart, and clean child process trees.
- Predecessors: H13-E.
- Entry: lifecycle protocol exists.
- Allowed scope: Windows Job Objects and fake child executables.
- Forbidden scope: orphanable children, unlimited restart, model logic.
- Deliverables: supervisor and cleanup evidence.
- Automated validation: crash, hang, descendant, shutdown, restart-bound tests.
- Manual validation: Task Manager process-tree audit.
- Pass criteria: no descendant survives main-process exit.
- Failure edges: stop integration and repair H16.
- Successors: H30-E.

## Native audio

### H20-E — Device Enumeration

- Purpose: enumerate and select CPAL/WASAPI endpoints and formats.
- Predecessors: H14-E.
- Entry: RT primitives pass.
- Allowed scope: discovery and deterministic format selection.
- Forbidden scope: persistent streams or default-device assumptions.
- Deliverables: enumeration adapter and selection policy.
- Automated validation: synthetic format negotiation tests.
- Manual validation: Ahmad's device/headphone inventory.
- Pass criteria: usable and unsupported configurations are explicit.
- Failure edges: hardware-blocked branch or H20 repair.
- Successors: H21-E and H22-E.

### H21-E — Microphone Capture

- Purpose: bounded callback-to-capture flow.
- Predecessors: H20-E.
- Entry: supported input format selected.
- Allowed scope: fixed conversion/copy, try-push, atomics.
- Forbidden scope: callback allocation, locks, logs, async, resampling,
  inference, filesystem, UI, state mutation.
- Deliverables: capture adapter and audit.
- Automated validation: callback, overrun, stream-error tests.
- Manual validation: microphone continuity/level check.
- Pass criteria: callback invariants hold and discontinuities are observable.
- Failure edges: H14-E or H20-E.
- Successors: H23-E.

### H22-E — Playback

- Purpose: bounded playback with exact rendered-frame evidence.
- Predecessors: H20-E.
- Entry: supported output format selected.
- Allowed scope: try-pop, silence, fixed conversion, atomic accounting.
- Forbidden scope: callback decisions, allocation, locks, logs, resampling,
  ledger mutation.
- Deliverables: playback adapter and render acknowledgements.
- Automated validation: underrun, partial block, stop, epoch-change tests.
- Manual validation: headphone playback and shutdown.
- Pass criteria: evidence supports exact ledger reconciliation.
- Failure edges: H14-E or H20-E.
- Successors: H23-E.

### H23-E — Off-Callback Resampling

- Purpose: bounded rate conversion outside callbacks.
- Predecessors: H21-E and H22-E.
- Entry: actual stream formats known.
- Allowed scope: off-callback conversion and measurement.
- Forbidden scope: callback resampling or unbounded historical buffering.
- Deliverables: resampler stages.
- Automated validation: rate, length, drift, quality, epoch-reset tests.
- Manual validation: audible artifact check.
- Pass criteria: correct bounded conversion preserves discontinuities.
- Failure edges: revise H23 or owning audio predecessor.
- Successors: H24-E.

### H24-E — Overrun and Device Recovery

- Purpose: recover toward recent clean audio after discontinuity.
- Predecessors: H23-E.
- Entry: streams expose counters and epochs.
- Allowed scope: stale-backlog discard, `AudioEpoch` advance, reset/restart.
- Forbidden scope: indefinite backlog preservation.
- Deliverables: freshness-first recovery policy.
- Automated validation: forced overrun, loss, reopen, stale-discard tests.
- Manual validation: overload and unplug/replug trial.
- Pass criteria: bounded recovery resumes recent clean audio.
- Failure edges: H14/H21/H23/H24 repair.
- Successors: H30-E.

## Fake vertical slice and Gate 0

### H30-E — Application Coordinator

- Purpose: serialize events through reducer and interpret effects.
- Predecessors: H12, H13, H14, H15, H16, H24.
- Entry: mandatory spine/audio join passes.
- Allowed scope: Tokio control plane, adapter wiring, test clock.
- Forbidden scope: infrastructure state mutation or callback reducer access.
- Deliverables: coordinator and effect runner.
- Automated validation: ordering, completion, cancellation, shutdown tests.
- Manual validation: state-ownership audit.
- Pass criteria: one authoritative event/reducer path.
- Failure edges: earliest owning predecessor.
- Successors: H31-E.

### H31-E — Fake End-to-End Conversation

- Purpose: cross microphone, fake workers, and playback boundaries.
- Predecessors: H30-E.
- Entry: coordinator and fakes are deterministic.
- Allowed scope: fake conversation and diagnostics.
- Forbidden scope: real models, memory intelligence, polished behavior.
- Deliverables: complete fake turn.
- Automated validation: scripted speech-to-render scenario.
- Manual validation: headphone trial.
- Pass criteria: no architectural boundary is bypassed.
- Failure edges: relevant H13-H30 node.
- Successors: H32-E, H33-E, H34-E.

### H32-E — Cancellation and Urgent Control

- Purpose: bounded stop, shutdown, fatal-audio, and barge-in.
- Predecessors: H31-E.
- Entry: active generation is observable.
- Allowed scope: allocation-free generation-aware publication.
- Forbidden scope: Boolean-only barge-in or callback blocking.
- Deliverables: urgent path and latency evidence.
- Automated validation: cancellation race matrix.
- Manual validation: repeated responsiveness trial.
- Pass criteria: controls carry observed generation and meet declared bounds.
- Failure edges: H14-E or H30-E.
- Successors: H35-E.

### H33-E — Stale-Output Rejection

- Purpose: prove `GenerationEpoch` as the sole freshness fence.
- Predecessors: H31-E.
- Entry: fake outputs can be delayed and reordered.
- Allowed scope: exhaustive generation-race tests.
- Forbidden scope: acceptance by request, turn, arrival, or worker identity.
- Deliverables: stale-rejection evidence.
- Automated validation: reorder across cancel, restart, new generation.
- Manual validation: transition trace review.
- Pass criteria: stale assistant text/audio never reaches playback.
- Failure edges: H10/H11/H12.
- Successors: H35-E.

### H34-E — Phrase/Playback Ledger

- Purpose: distinguish every phrase lifecycle state.
- Predecessors: H31-E and H22-E.
- Entry: playback reports rendered progress.
- Allowed scope: pure ledger transitions.
- Forbidden scope: treating queued, synthesized, or interrupted text as heard.
- Deliverables: ledger and reconciliation tests.
- Automated validation: full render, discard, partial interruption, shutdown.
- Manual validation: ledger trace versus audible output.
- Pass criteria: only fully rendered text is classified heard.
- Failure edges: H22-E or H11/H12.
- Successors: H35-E.

### H35-E — Fault Matrix

- Purpose: adversarially join audio, workers, freshness, ledger, supervision.
- Predecessors: H32, H33, H34.
- Entry: targeted behavior suites pass.
- Allowed scope: hostile failure injection and repair routing.
- Forbidden scope: flaky waivers or exit-code-only evidence.
- Deliverables: fault matrix report.
- Automated validation: crash, hang, malformed IPC, pressure, overrun, device
  loss, cancellation race, shutdown storm.
- Manual validation: adversarial invariant review.
- Pass criteria: declared outcomes hold for the matrix.
- Failure edges: H35-R.
- Successors: H36-E.

### H35-R — Gate 0 Architecture Rollback

- Purpose: repair the earliest invariant owner exposed by H35/H36.
- Predecessors: failed H35-E or H36-E.
- Entry: minimized reproducer and ownership diagnosis.
- Allowed scope: smallest owning-boundary correction.
- Forbidden scope: assertion weakening, symptom patches, real models.
- Deliverables: root-cause report and repair.
- Automated validation: reproducer, owner suite, full fault matrix.
- Manual validation: invariant-restoration review.
- Pass criteria: reproducer and matrix pass.
- Failure edges: blocked human architecture decision.
- Successors: H35-E then H36-E.

### H36-E — Gate 0 Soak Decision

- Purpose: authorize or reject real-model work.
- Predecessors: H35-E.
- Entry: fault matrix green.
- Allowed scope: extended fake soak and resource measurement.
- Forbidden scope: model downloads or inference.
- Deliverables: GO/NO-GO evidence.
- Automated validation: memory, handles, queues, cleanup, freshness, shutdown.
- Manual validation: responsiveness and process audit.
- Pass criteria: all thresholds pass and decision is GO.
- Failure edges: H35-R.
- Successors: H37-E only after GO.

## Benchmark foundation

### H37-E — Benchmark Harness and Baseline

- Purpose: establish reproducible conversation, latency, memory, audio, and
  stress measurements.
- Predecessors: H36-E GO.
- Entry: fake architecture is stable.
- Allowed scope: harness, synthetic fixtures, metadata, baseline format.
- Forbidden scope: personal raw audio commits or undeclared model selection.
- Deliverables: benchmark tree and fake/native-audio baseline.
- Automated validation: repeatability, schema, cold/warm runs, percentiles.
- Manual validation: perceived-behavior correlation.
- Pass criteria: documented hardware/configuration reproduces baselines.
- Failure edges: repair measurement method.
- Successors: H40, H42, H43, H44, H45, H46 as entries permit.

## Real components and product systems

### H40-E — llama-server Transport

- Purpose: replaceable local LLM adapter.
- Predecessors: H37-E.
- Entry: acquisition authorized and capacity checked.
- Allowed scope: local transport, streaming, cancellation, supervision.
- Forbidden scope: core prompts, cloud requirement, public service, silent
  downloads.
- Deliverables: adapter and fake-server tests.
- Automated validation: stream, timeout, malformed, crash, stale tests.
- Manual validation: cleanup and security review.
- Pass criteria: protocol and freshness invariants hold.
- Failure edges: H4R-E.
- Successors: H41-E.

### H41-E — Philosopher-Model Benchmark

- Purpose: select a reflective local model.
- Predecessors: H40-E and H37-E.
- Entry: corpus and thresholds declared.
- Allowed scope: authorized candidates and repeatable comparison.
- Forbidden scope: coding bias, paid requirement, model coupling.
- Deliverables: scored selection and fallback.
- Automated validation: latency, RAM, stability, cancellation.
- Manual validation: Conversation Principles and philosophical quality.
- Pass criteria: technical and human thresholds pass.
- Failure edges: H4R-E or blocked human model decision.
- Successors: H47-E, H49-E, H50-E.

### H42-E — ASR Benchmark

- Purpose: select local ASR for Ahmad's natural English.
- Predecessors: H37-E.
- Entry: representative consented corpus and criteria.
- Allowed scope: local candidate comparison.
- Forbidden scope: cloud-only dependency or grammar-driven distortion.
- Deliverables: selected adapter and fallback.
- Automated validation: accuracy proxy, latency, cancellation, resources.
- Manual validation: semantic usefulness.
- Pass criteria: accuracy and latency thresholds pass.
- Failure edges: H4R-E.
- Successors: H43-E, H50-E.

### H43-E — VAD and Endpointing

- Purpose: deterministic in-process utterance boundary policy.
- Predecessors: H24-E, H37-E, H42-E.
- Entry: clean epoch-aware audio and ASR behavior known.
- Allowed scope: signal features and deterministic state machine.
- Forbidden scope: callback inference or grammar-driven interruptions.
- Deliverables: endpoint policy and corpus tests.
- Automated validation: noise, pauses, discontinuity, barge-in, timing.
- Manual validation: reflective-speech trials.
- Pass criteria: declared false/missed endpoint bounds.
- Failure edges: H43 repair or H24.
- Successors: H50-E.

### H44-E — TTS Benchmark and Adapter

- Purpose: select comfortable, interruptible local TTS.
- Predecessors: H37-E.
- Entry: voice, latency, and licensing criteria.
- Allowed scope: local candidates and ledger-compatible adapter.
- Forbidden scope: paid requirement or synthesis-as-heard.
- Deliverables: selection and fallback.
- Automated validation: first audio, cancellation, format, crash, stale output.
- Manual validation: long-listening comfort.
- Pass criteria: technical and human thresholds pass.
- Failure edges: H4R-E.
- Successors: H50-E.

### H45-E — Ratatui Interface

- Purpose: first usable native terminal UI.
- Predecessors: H30-E and H37-E.
- Entry: stable projections and typed intentions.
- Allowed scope: rendering, input, status, fault views.
- Forbidden scope: state mutation, worker calls, browser UI.
- Deliverables: TUI.
- Automated validation: rendering, resize, input, restoration.
- Manual validation: readability and keyboard flow.
- Pass criteria: UI remains a projection/input adapter.
- Failure edges: H30 or H45 repair.
- Successors: H50-E.

### H46-E — SQLite Persistence

- Purpose: explicit local persistence and privacy policy.
- Predecessors: H12-E and H37-E.
- Entry: retention, location, deletion, backup approved.
- Allowed scope: SQLite adapter and migrations.
- Forbidden scope: cloud sync, direct worker writes, hidden retention.
- Deliverables: storage and recovery evidence.
- Automated validation: migration, transaction, corruption, deletion, restart.
- Manual validation: stored-content/privacy review.
- Pass criteria: persistence is explicit, reducer-mediated, recoverable.
- Failure edges: blocked privacy decision or H46 repair.
- Successors: H47-E, H48-E, H49-E, H50-E.

### H47-E — Memory Engine

- Purpose: implement the first-class memory-quality pipeline.
- Predecessors: H41-E and H46-E.
- Entry: provenance, retention, review, scoring policy declared.
- Allowed scope: candidates, ranking, duplicates, conflicts, consolidation,
  timeline, retrieval policy.
- Forbidden scope: storing everything, silent belief replacement, direct SQL,
  LLM-owned acceptance.
- Deliverables: `hector-memory`, tests, reviewable proposals.
- Automated validation: duplicates, conflicts, provenance, consolidation,
  deletion, ranking, retrieval latency.
- Manual validation: Ahmad reviews fidelity and belief changes.
- Pass criteria: only accepted provenance-bearing memories persist.
- Failure edges: revise policy/model/storage boundary.
- Successors: H49-E, H50-E.

### H48-E — Book Subsystem

- Purpose: model Ahmad's reading world separately from personal memory.
- Predecessors: H46-E and H41-E when semantic extraction is used.
- Entry: copyright, quotation, ingestion, metadata policy declared.
- Allowed scope: books, authors, progress, themes, characters, notes, lawful
  short quotations, cross-references.
- Forbidden scope: unauthorized bulk text, treating books as personal memory,
  direct model storage.
- Deliverables: `hector-knowledge::books` and persistence mappings.
- Automated validation: catalog, progress, provenance, cross-reference,
  deletion, budget.
- Manual validation: fidelity to books and notes.
- Pass criteria: sources remain separate, lawful, provenance-aware.
- Failure edges: blocked policy decision or H48 repair.
- Successors: H49-E, H50-E.

### H49-E — Knowledge Engine

- Purpose: build bounded context across memory, books, and history.
- Predecessors: H41-E, H47-E, H48-E.
- Entry: sources and prompt budgets defined.
- Allowed scope: retrieval plans, ranking, time comparison, context assembly.
- Forbidden scope: direct I/O, unbounded prompts, hidden memory mutation,
  LLM-owned retrieval.
- Deliverables: `hector-knowledge` and Knowledge Context.
- Automated validation: budgets, determinism, source diversity, latency,
  conflict visibility.
- Manual validation: relevance and continuity review.
- Pass criteria: context is traceable, bounded, relevant, preassembled.
- Failure edges: H47/H48/H49 or H41.
- Successors: H50-E.

### H50-E — Conversation Features and Real Integration

- Purpose: integrate dialogue, knowledge, English/session reflection, and real
  workers.
- Predecessors: H41 through H49 as applicable.
- Entry: component contracts and Conversation Principles pass.
- Allowed scope: `hector-features` and real vertical integration.
- Forbidden scope: orchestration in features, live grammar policing, generic
  assistant drift, persistence bypass.
- Deliverables: dialogue, delayed English reflection, session reflection, real
  integrated slice.
- Automated validation: principle scenarios, separation, opt-out, freshness,
  context budgets, ledger, fault regression.
- Manual validation: sustained conversation judged by Ahmad.
- Pass criteria: principles and all ownership boundaries survive integration.
- Failure edges: earliest owning component or H35-R for systemic faults.
- Successors: H51-E.

### H4R-E — Real Runtime Reselection

- Purpose: replace a failed ASR, LLM, TTS, or runtime candidate.
- Predecessors: failed H40, H41, H42, or H44.
- Entry: failure belongs to candidate/adapter, not a hidden core defect.
- Allowed scope: candidate-specific replacement and benchmark repetition.
- Forbidden scope: weakening protocol, freshness, privacy, ledger, RT rules.
- Deliverables: comparison and replacement evidence.
- Automated validation: component benchmark and Gate 0 regression.
- Manual validation: quality and maintenance comparison.
- Pass criteria: unchanged thresholds pass.
- Failure edges: blocked human model decision.
- Successors: corresponding real-component node.

### H51-E — Regression Benchmark Gate

- Purpose: detect behavioral and performance regression in the real system.
- Predecessors: H50-E.
- Entry: real integration functionally passes.
- Allowed scope: full benchmark matrix and evidence-based threshold review.
- Forbidden scope: deleting adverse results, unjustified baseline changes,
  average-only reporting.
- Deliverables: versioned benchmark report and decision.
- Automated validation: startup, ASR, first token, TTS, interruption, memory,
  knowledge, RAM, handles, processes, overruns, stress, soak.
- Manual validation: perceived responsiveness/quality correlation.
- Pass criteria: hard thresholds pass; soft exceptions are explicitly accepted.
- Failure edges: return to owning node and repeat H51.
- Successors: H53-E; optional H52-E.

### H52-E — Optional Container Worker Profile

- Purpose: prove container workers preserve native application semantics.
- Predecessors: H51-E PASS and explicit authorization.
- Entry: native mode passes and container tools are verified.
- Allowed scope: optional compose/profile, private network, local mounts,
  adapter configuration.
- Forbidden scope: mandatory Docker, public bindings, domain changes, cloud
  dependency, Gate 0 replacement.
- Deliverables: optional deployment profile and comparison.
- Automated validation: protocol parity, freshness, cancellation, cleanup,
  startup, latency, mounts.
- Manual validation: Windows burden and privacy.
- Pass criteria: behavior preserved within declared optional overhead.
- Failure edges: reject/remove optional profile; native mode remains valid.
- Successors: optional maintenance only; H53 does not depend on H52.

### H53-E — Personal Acceptance and Readiness

- Purpose: authorize durable native personal use.
- Predecessors: H51-E PASS.
- Entry: functional, fault, privacy, benchmark gates pass.
- Allowed scope: extended acceptance and documentation finalization.
- Forbidden scope: public distribution, installers, accounts, cloud sync,
  public APIs, commercial qualification, speaker AEC.
- Deliverables: personal-use readiness evidence.
- Automated validation: workspace, faults, benchmarks, persistence, cleanup,
  soak.
- Manual validation: sustained conversation, memory/knowledge review, delayed
  English behavior, recovery, listening comfort.
- Pass criteria: behavioral criteria pass and Ahmad accepts the experience.
- Failure edges: earliest owning node.
- Successors: future private-maintenance graph.

## Graph classifications

- Parallel branches: H10/H13/H14/H16; H21/H22; H32/H33/H34; eligible
  H40/H42/H43/H44/H45/H46; memory/books when entries pass.
- Mandatory joins: H04, H30, H35, H36, H50, H51, H53.
- Decision nodes: H02, H36, H41, H42, H44, H46 policy, H51, H53.
- Rollback nodes: H02-R, H35-R, H4R-E.
- Optional non-gating branch: H52-E.
- Blocked branches: unrepairable MSVC/SDK, unsupported audio hardware,
  unresolved privacy/copyright policy, inadequate disk, no acceptable local
  model, or unrecoverable Gate 0 invariant failure.
