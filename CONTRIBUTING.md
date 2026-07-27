# Contributing to Hector

Hector is a private personal tool, but its repository should remain legible and
maintainable over years. Work is organized by the directed graph in
`docs/graph/MASTER_BUILD_GRAPH.md`.

## Before changing anything

1. Identify the single active graph node.
2. Load `AGENTS.md` and the governing documents it names.
3. Demonstrate every entry condition.
4. Restate the current repository state, assumptions, allowed scope, and
   forbidden scope.
5. Inspect unrelated working-tree changes and preserve them.

## Windows toolchain

The reference build target is `x86_64-pc-windows-msvc`. A future
repository-local toolchain file will select it only after H02-E proves that the
MSVC linker and Windows SDK can compile, link, and run a disposable native
program. The currently installed GNU default is not acceptable evidence for a
Hector build.

Do not silently install or update Rust, Visual Studio, SDK components, CMake,
Ninja, Docker, worker runtimes, or models. Installation requires the owning
graph node and explicit authority.

## Change loop

Use the full loop in `docs/governance/EXECUTION_LOOP.md`:

- implement the smallest coherent change;
- format node-owned files;
- run targeted behavioral tests;
- run relevant workspace-wide checks;
- inspect the complete diff;
- compare with architecture and conversation invariants;
- repair and repeat;
- write a structured evidence report;
- stop at the node boundary.

An exit code of zero does not establish behavioral correctness.

## Rust validation

Once the Cargo workspace exists, the normal validation set is:

- format check;
- workspace compilation/check on MSVC;
- Clippy with repository policy;
- targeted tests;
- relevant workspace-wide tests;
- dependency-direction checks;
- graph-node-specific fault or benchmark tests.

Commands and exact observed outcomes belong in the node evidence report.

## Commits and branches

Keep commits scoped to one graph node or one coherent repair. Do not modify Git
configuration or rename branches as an incidental task. Do not begin a
successor node merely to make a commit look more complete.

## Never commit

- credentials, tokens, secrets, or private keys;
- model weights or third-party runtime binaries;
- personal conversations, raw microphone recordings, generated speech, or
  SQLite databases;
- private benchmark fixtures or unredacted evidence;
- build artifacts, logs, crash dumps, caches, or machine-specific paths;
- container volumes or copied model directories.

When runtime artifacts first exist, their exclusion policy must be introduced
by the graph node that creates them, not speculatively.

## Documentation and decisions

Update governing documents when an architecture or product decision changes.
Record durable tradeoffs as decision records. Never edit historical decision
records to disguise a reversal; supersede them with a new record.
