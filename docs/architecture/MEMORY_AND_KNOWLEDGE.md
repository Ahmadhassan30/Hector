# Memory and Knowledge Architecture

Memory and knowledge are defining Hector capabilities. They are separate from
runtime orchestration, storage mechanics, and model prompting.

## Memory Engine

The Memory Engine is a deterministic policy boundary:

```text
conversation evidence
  → candidate extraction
  → importance ranking
  → duplicate detection
  → conflict detection
  → consolidation proposal
  → reducer acceptance
  → persistence effect
  → retrieval-index update
```

The engine may use an LLM adapter to propose candidates in later nodes, but an
LLM response is evidence, not an accepted memory. The main reducer owns
acceptance and persistence effects.

### Required properties

- Every memory has source provenance and relevant session/turn references.
- Importance is explicit and testable.
- Duplicates are merged only under a declared rule.
- Conflicts remain visible until a policy or Ahmad resolves them.
- Changing beliefs are represented over time rather than overwritten.
- Consolidation produces a reviewable proposal.
- Deletion removes the record and its retrieval representation.
- Retrieval never implies that a memory is certain or current.
- Storing every conversational detail is a failure.

SQLite implements records and transactions but cannot decide memory quality.

## Knowledge Engine

The Knowledge Engine assembles context before an LLM invocation:

```text
dialogue intention
  → retrieval plan
  → thematic memory retrieval
  → book retrieval
  → conversation-history retrieval
  → comparison over time
  → source-aware ranking
  → prompt-budget enforcement
  → Knowledge Context
```

The Knowledge Context contains selected material, provenance, relevance
reasoning suitable for diagnostics, conflict markers, and explicit budget use.
Model adapters convert it into runtime-specific requests. The LLM cannot expand
the retrieval budget or silently fetch additional private material.

## Book subsystem

Books are not personal memories. The initial subsystem lives under
`hector-knowledge::books` and owns:

- books and authors;
- Ahmad's reading progress;
- themes and characters;
- Ahmad-authored notes;
- provenance-bearing, legally appropriate short quotations;
- cross-references between books, themes, conversations, and memories.

Bulk copyrighted text is not retained without clear authority. Quotations must
preserve source and location when known. A book claim cannot be attributed to
Ahmad merely because it was retrieved during his conversation.

The subsystem becomes a separate crate only if independent ingestion,
indexing, reuse, or testing demonstrates a real boundary.

## Ports and persistence

Memory and knowledge algorithms perform no direct I/O. Runtime adapters fetch
candidate records through concrete ports, invoke deterministic policies, and
return proposed effects/events. `hector-storage` maps accepted memory and book
records to SQLite.

Do not create persistence, embeddings, indexes, or traits until their owning
nodes establish requirements. The architecture supports lexical, vector, or
hybrid retrieval later without making one method a domain dependency.

## Product use

Dialogue receives a completed, bounded Knowledge Context. Delayed English
reflection and session reflection may receive their own narrower contexts.
Retrieval is justified only when it deepens the present exchange; recalling
facts merely to demonstrate memory violates the Conversation Principles.

## Evaluation

H47-E validates memory quality, conflicts, provenance, consolidation, deletion,
and retrieval latency. H48-E validates books, citations, progress, and
cross-references. H49-E validates relevance, source diversity, conflict
visibility, determinism, context latency, and hard budget enforcement. H51-E
compares them with recorded baselines.
