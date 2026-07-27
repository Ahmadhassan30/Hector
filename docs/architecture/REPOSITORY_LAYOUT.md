# Repository Layout

## Decision

Hector uses a balanced, product-aware Rust workspace. It isolates the pure core,
worker protocol, hard real-time audio, orchestration, Windows mechanics,
storage, and UI without creating a crate for every internal concept.

Product crates appear only when their owning behavior is implemented:
`hector-memory` at H47-E, `hector-knowledge` at H49-E, and
`hector-features` at H50-E. Books begin as an isolated subsystem inside
knowledge and become a crate only if an independent boundary emerges.

## Mature target

```text
Hector/
├── apps/hector/
├── crates/
│   ├── hector-core/
│   ├── hector-protocol/
│   ├── hector-audio/
│   ├── hector-runtime/
│   ├── hector-platform-windows/
│   ├── hector-storage/
│   ├── hector-tui/
│   ├── hector-memory/       # H47-E
│   ├── hector-knowledge/    # H49-E, includes books/
│   └── hector-features/     # H50-E
├── workers/
├── benchmarks/
├── tests/
├── deploy/container/        # optional H52-E only
├── docs/
└── scripts/
```

This tree is a destination, not a bootstrap checklist. A directory or crate is
created only when its first real node needs it. Empty placeholders are
forbidden.

## Top-level ownership

- `apps/`: executable composition roots only.
- `crates/`: Rust ownership boundaries.
- `workers/`: future worker implementations or launch specifications; never
  committed model weights.
- `benchmarks/`: repeatable performance and behavioral comparisons.
- `tests/`: cross-crate protocol, vertical, fault, and soak validation.
- `deploy/container/`: optional post-native worker profile.
- `docs/`: product, architecture, decisions, graph, and governance.
- `scripts/`: repeatable automation introduced on first use.

## Crate responsibilities

- `hector-core`: pure identifiers, state, events, effects, reducer, ledger.
- `hector-protocol`: bounded versioned worker DTOs and framing.
- `hector-audio`: callback boundary, CPAL devices/streams, resampling, recovery.
- `hector-runtime`: coordinator, effect runner, worker clients, concrete ports,
  clocks, metrics.
- `hector-platform-windows`: Job Objects and OS process primitives.
- `hector-storage`: SQLite records, transactions, and migrations.
- `hector-tui`: immutable view projections and typed input intentions.
- `hector-memory`: deterministic candidate-to-retrieval memory lifecycle.
- `hector-knowledge`: retrieval plans, books, comparisons, context budgets.
- `hector-features`: dialogue, delayed English reflection, session reflection.

## Allowed dependency direction

```text
hector-core
  ↑       ↑        ↑
audio   protocol  memory → knowledge → features

storage → core + memory + knowledge record types
tui → core projections
platform-windows → no product crate
runtime → core + protocol + audio + platform-windows + storage
          + memory + knowledge + features
apps/hector → runtime + tui
```

The arrows indicate dependency toward the item at the arrowhead in prose:
audio/protocol/memory may depend on core; knowledge may depend on core and
memory; features may depend on core and knowledge.

Forbidden edges include:

- core to Tokio, CPAL, Ratatui, SQLite, Windows APIs, models, or containers;
- memory or knowledge to SQLite or worker adapters;
- features to storage, audio, Windows APIs, or worker implementations;
- TUI or infrastructure directly mutating application state;
- audio callbacks entering runtime or core reducer code;
- domain/product crates depending on Docker or public HTTP.

## Alternatives considered

### Broad layout

`apps/hector`, `hector-core`, and `hector-platform` optimize early speed but
leave audio, process, persistence, and orchestration boundaries too dependent
on convention.

### Granular layout

Separate crates for identifiers, reducer, effects, audio types, audio control,
each memory stage, books, dialogue, English reflection, and test utilities
maximize isolation but freeze immature APIs and burden one-developer
maintenance.

The balanced layout protects high-risk boundaries while keeping evolving
policies as modules within clear owners.
