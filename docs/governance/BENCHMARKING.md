# Benchmarking Governance

Benchmarks protect Hector's felt responsiveness and conversational quality.
They are separate from correctness tests and never replace them.

## Benchmark families

Future H37-E creates, on first use:

```text
benchmarks/
├── conversation/
├── latency/
├── memory/
├── audio/
├── stress/
└── baselines/
```

H01-E creates no benchmark directories or implementation.

## Required measurements

- cold and warm startup time;
- ASR partial and final latency;
- LLM first-token and completion latency;
- TTS first-audio and completion latency;
- interruption-to-silence latency;
- memory candidate/retrieval latency;
- Knowledge Context construction latency;
- audio callback pressure, underruns, and overruns;
- process count, handle count, and RAM use;
- queue depth and resource growth during soak;
- conversation-principle scenario outcomes.

Report distributions and at least median, p95, and worst observed value for
latency-sensitive measurements. Averages alone are insufficient.

## Reproducibility metadata

Each baseline records:

- date and graph node;
- commit or exact worktree identity;
- Windows build;
- CPU, RAM, and relevant audio device;
- Rust/tool versions;
- native or optional container deployment;
- worker runtime/model names, versions, quantization, and settings;
- warm-up policy, iteration count, and fixture identity;
- power/thermal conditions when material.

## Cold, warm, and soak runs

Cold and warm results are separate. Setup and model-loading time cannot be
silently removed from startup numbers. Soak reports record duration and
time-series resource behavior rather than only final values.

## Baseline policy

- Establish the measurement method and thresholds before using results to
  choose an implementation.
- Never delete or overwrite an adverse baseline.
- A baseline change requires a reason, comparison, and graph-node evidence.
- Hard thresholds fail the node.
- Soft regressions require explicit documented acceptance and must not be
  disguised by retuning the fixture.
- H51-E is the mandatory real-system regression gate.
- Optional container results are compared with native results but cannot make
  native mode optional.

## Behavioral evaluation

Conversation benchmarks use scenarios derived from
`../product/CONVERSATION_PRINCIPLES.md`, including disagreement, uncertainty,
imperfect English, reflective pauses, temptation to over-summarize, memory
conflicts, and delayed language reflection. Automated scoring may assist but
cannot replace Ahmad's evaluation of philosophical quality.

## Privacy

Use synthetic, public-domain, licensed, or explicitly consented fixtures.
Personal raw audio and private conversations remain outside Git. Published or
committed results must not contain recoverable personal content, secrets,
machine credentials, or model weights.

## Validity

A faster result is not a pass if correctness, freshness, playback truth,
privacy, or Conversation Principles regress. A command completing successfully
does not prove the benchmark measured the intended path.
