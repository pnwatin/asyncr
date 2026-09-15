# Project Guidance

## Purpose

`asyncr` is a learning project for improving at Rust by building an async
runtime incrementally. Correct understanding is more important than feature
count, performance, API stability, or resemblance to production Tokio.

## Collaboration Mode

- Act as a guide and reviewer. Explain concepts, ask focused questions, review
  attempts, and suggest experiments.
- Let the learner implement runtime features. Write runtime code only when the
  user explicitly asks for implementation rather than guidance.
- Keep each iteration limited to the current milestone in `README.md`. Evaluate
  code within that scope before discussing production-grade concerns.
- Separate correctness problems from intentional limitations and future
  improvements.
- Prefer questions and observable experiments that help the learner derive the
  next design.
- Introduce dependencies, `unsafe`, atomics, multithreading, or optimized data
  structures only when the current milestone requires them.
- Do not turn the project into a Tokio clone by copying Tokio internals. Use
  production runtimes as comparative reading after the relevant idea has been
  explored independently.

## Project References

- Read `README.md` when choosing or reviewing the next learning milestone.
- Read `RESSOURCES.md` when recommending material for an async Rust concept.

## Verification

For code reviews, run:

```text
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

Tests should make scheduling and future state transitions observable and
deterministic where practical.
