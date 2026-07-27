# ADR 0002: Native Windows Default with Optional Containers

- Status: Accepted for future implementation after H51-E
- Date: 2026-07-28

## Context

Hector is Windows-native and local-first. Native worker processes provide the
least operational overhead and clearest integration with Windows Job Objects.
Containers may later make selected ASR, LLM, or TTS runtimes easier to package
or replace, but Docker must not become a prerequisite for the personal tool.

## Decision

Native Windows execution is the reference, required, and benchmarked mode.

After the native real-component integration and H51-E regression gate pass,
H52-E may add an optional container worker profile. The main application uses
the same conceptual worker ports and protocol regardless of deployment.

An optional profile must:

- preserve `GenerationEpoch` freshness and generation-aware cancellation;
- preserve phrase-ledger and playback acceptance semantics;
- bind only to loopback or a private container network by default;
- keep model weights in explicit local mounts outside Git;
- expose diagnostics separately from protocol data;
- include startup, latency, cleanup, and resource comparisons with native mode;
- remain removable without changing domain, memory, knowledge, or features.

## Rejected alternatives

### Containers as the default

Rejected because it makes Docker Desktop, virtualization, networking, volume
management, and additional resource overhead prerequisites for a private
Windows-native tool.

### No container support under any circumstances

Rejected because a later worker candidate may have a substantially better
maintained container distribution. The adapter architecture can support this
without contaminating the domain.

### Public worker HTTP services

Rejected. Hector does not need a public API. Any local HTTP transport used by
an upstream runtime is an internal adapter detail and must not bind publicly by
default.

## Consequences

- Gate 0 and native acceptance cannot depend on Docker.
- Container tooling is not inspected or installed during the foundation.
- H52-E is non-gating for H53-E personal acceptance.
- If container parity, privacy, cleanup, or overhead is unacceptable, the
  profile is rejected while native mode remains valid.

## Revisit conditions

Reconsider only after native mode passes and benchmark evidence shows a clear
maintenance or capability benefit. Any move away from native-default requires a
new ADR and explicit human decision.
