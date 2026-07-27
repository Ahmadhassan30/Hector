# Hector

Hector is Ahmad Hassan's private, local-first, Windows-native companion for
deep spoken-English conversation about philosophy, literature, human nature,
meaning, identity, ambition, loneliness, love, mortality, faith and doubt,
beauty, and technology's effect on humanity.

Hector is currently in its documentation and architecture-foundation phase.
It does not yet contain an application, Cargo workspace, model integration, or
conversation runtime.

The intended system is a native Rust application. The main process will own
audio, timing, orchestration, state, cancellation, playback acceptance,
persistence policy, process supervision, and a Ratatui interface. Replaceable
local workers may later provide ASR, LLM, and TTS inference. No model work may
begin until the fake-worker architecture gate passes.

Hector is not a coding assistant, enterprise application, public SaaS product,
generic voice-command assistant, conventional grammar tutor, or disposable
MVP. Public distribution, accounts, cloud sync, public APIs, speaker-mode AEC,
and commercial release qualification are outside the current graph.

## Governing documents

- [Product intent](docs/product/PRODUCT.md)
- [Conversation Principles](docs/product/CONVERSATION_PRINCIPLES.md)
- [Architecture](docs/architecture/ARCHITECTURE.md)
- [Architecture invariants](docs/architecture/INVARIANTS.md)
- [Repository layout](docs/architecture/REPOSITORY_LAYOUT.md)
- [Memory and knowledge](docs/architecture/MEMORY_AND_KNOWLEDGE.md)
- [Master build graph](docs/graph/MASTER_BUILD_GRAPH.md)
- [Execution loop](docs/governance/EXECUTION_LOOP.md)
- [Benchmarking policy](docs/governance/BENCHMARKING.md)
- [Contribution workflow](CONTRIBUTING.md)

The next graph node after this documentation foundation is H02-E, environment
verification. Cargo workspace creation remains blocked until an x64 MSVC
compile/link/run check passes.
