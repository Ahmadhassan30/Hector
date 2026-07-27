# Execution Loop

Every future execution node uses this closed loop.

## Required loop

1. **Inspect** repository, environment, predecessor evidence, and current diff.
2. **Restate** current state, assumptions, entry conditions, allowed scope, and
   forbidden scope.
3. **Load governing documents** in the order defined by `../../AGENTS.md`.
4. **Propose the smallest coherent implementation** that can satisfy the node.
5. **Perform a pre-mortem** covering likely invariant, platform, freshness,
   cancellation, cleanup, privacy, and rollback failures.
6. **Implement** only the active node.
7. **Format** only node-owned files.
8. **Compile/check** with the repository-selected MSVC toolchain.
9. **Run targeted tests** for declared behavior.
10. **Run relevant workspace-wide tests**.
11. **Inspect the complete diff**, status, dependencies, and generated files.
12. **Compare against invariants** and Conversation Principles where relevant.
13. **Repair defects** at the earliest owning boundary.
14. **Repeat checks** until behavior is green or the node is honestly blocked.
15. **Write structured evidence**.
16. **Stop at the node boundary** without starting a successor.

Successful process exit is not sufficient evidence. A node passes only when its
declared observable criteria and manual validations pass.

## Repair loop

```text
preserve and minimize the failure
  → identify earliest owning boundary
  → repair the smallest coherent scope
  → run targeted behavioral validation
  → run relevant workspace regression
  → compare benchmarks where applicable
  → inspect diff and invariants
  → repeat or declare BLOCKED
```

Do not weaken assertions, delete adverse fixtures, call a deterministic failure
flaky, or patch a later layer to conceal an earlier ownership defect.

## Adversarial sub-loop

H12, H13, H14, H16, H21-H24, H30, H32-H36, H40, H42-H44, H46-H51,
and other explicitly risky nodes require:

```text
implementation
  → invariant review
  → deterministic failure injection
  → root-cause repair
  → targeted revalidation
  → workspace regression
  → benchmark comparison where relevant
```

## Evidence report

Every report includes:

- node ID, baseline commit, and starting worktree state;
- predecessor and entry-condition proof;
- assumptions and deviations;
- changed files and dependencies;
- exact commands and relevant tool versions;
- behavioral criteria with observed results;
- manual validation observations;
- benchmark comparisons where applicable;
- complete diff/invariant review;
- known limitations and failure routing;
- final `PASS` or `BLOCKED` decision;
- confirmation that successor work did not begin.

Sensitive personal data is summarized or redacted. Evidence reports must not
commit private audio, conversations, model data, secrets, or databases.

## Blocking

Stop honestly when an entry condition, required authority, hardware condition,
privacy decision, or behavioral criterion cannot be satisfied. State the exact
blocker and its graph failure edge. Difficulty, slow work, or an undesirable
test result is not by itself a reason to bypass the loop.
