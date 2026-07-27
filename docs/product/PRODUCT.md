# Hector Product Intent

## Person and purpose

Hector is Ahmad Hassan's private, local-first, Windows-native philosophical
speaking companion. It is intended to become a durable personal tool for deep
spoken-English conversation rather than a public product.

The center of the experience is sustained conversation about philosophy,
literature, human nature, meaning, identity, ambition, loneliness, love,
mortality, faith and doubt, beauty, technology and humanity, Ahmad's books,
recurring questions, and changing beliefs.

The desired presence is humanistic, reflective, opinionated, intellectually
alive, honest about uncertainty, and capable of respectful disagreement.

## English development

English improvement occurs primarily through sustained natural speech and
generous interpretation of imperfect expression. Hector may clarify selectively
when meaning genuinely depends on it. After a discussion, it may offer a small
number of useful natural alternatives or observations.

Live philosophical thought must not be repeatedly interrupted by grammar
correction. Hector is not a conventional language lesson wrapped around a
conversation.

## Separate product systems

The following systems must remain distinct even when they cooperate:

1. **Live philosophical dialogue** maintains the present exchange.
2. **Thematic intellectual memory** retains reviewed, provenance-bearing
   themes, questions, and changing beliefs.
3. **Delayed English reflection** offers bounded language observations after
   the relevant discussion.
4. **Session reflection** proposes a reviewable synthesis of themes and open
   questions at a session boundary.

The Knowledge Engine supplies source-aware and budgeted context to dialogue.
The book subsystem remains distinct from autobiographical memory.

## Privacy and locality

The main Rust process owns application truth and storage policy. Local workers
may provide inference but cannot mutate state or write memory directly. No paid
runtime API is required. Private conversations and personal artifacts remain
local unless Ahmad explicitly authorizes a future change.

## Non-goals

Hector is not:

- a coding or technical-work assistant;
- an enterprise application;
- a public SaaS product;
- a generic command assistant;
- a public HTTP service;
- a browser, Electron, Tauri, React, or JavaScript UI;
- a disposable MVP;
- a system designed for multiple users.

The current graph also defers public distribution, signed installers, accounts,
cloud sync, public APIs, commercial release qualification, and speaker-mode
acoustic echo cancellation.

## Development gates

The first implementation gate uses fake ASR, LLM, and TTS workers. No model
weights or real inference runtimes may be introduced until fault tests and a
fake-system soak audit produce Gate 0 GO. Native Windows mode must pass before
any optional container worker profile is considered.
