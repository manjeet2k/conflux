---
name: superpowers
description: >-
  Use this skill to apply high-leverage engineering superpowers: test-driven invariant verification (TDD),
  scientific hypothesis-driven debugging, chaos and fault-injection testing, and mandatory quality gates.
  Activate when implementing new components, investigating failures, or preparing code reviews.
---

# Superpowers Engineering Skill

A collection of systematic, high-leverage methodologies designed to build robust, bug-free, and fault-tolerant software.

---

## Superpower 1: Test-Driven Invariants (TDD)

For every critical component in the download engine:
1. **Define the Invariants**:
   - *Chunk Invariant*: $\sum \text{chunk.size} == \text{total\_bytes}$, with no gaps and no overlapping offsets.
   - *Write Invariant*: Writing chunk $N$ at offset $O_N$ does not alter bytes at offset $O_M$.
   - *State Invariant*: Every chunk is in exactly one state: `Pending`, `Downloading`, `Completed`, or `Failed`.
2. **Write the Test Before Complex Logic**:
   - Write failing unit tests for edge conditions (0-byte files, 1-byte chunks, network disconnect mid-chunk).
   - Implement the minimal logic to make the test pass.
   - Refactor cleanly.

---

## Superpower 2: Scientific Hypothesis-Driven Debugging

When a test fails or a download encounters an error:
1. **Never guess or make random code edits.**
2. **Observe**: Collect exact logs, stack traces, and error codes.
3. **Formulate Hypothesis**: State: *"I hypothesize that the socket fails with WSAEADDRNOTAVAIL because the network adapter IP changed or is no longer bound to a physical link."*
4. **Isolate & Reproduce**: Write a minimal standalone script or targeted test that isolates only that behavior.
5. **Verify**: Test the hypothesis against evidence. If confirmed, fix the root cause and add a regression test.

---

## Superpower 3: Chaos & Fault-Injection Testing

A download accelerator must survive real-world network instability:
1. **Adapter Drop**: Simulate an adapter disconnecting while actively downloading a chunk.
   - *Expected Behavior*: The worker detects socket EOF/reset, releases the chunk back to the pending queue with status `Pending`, and surviving adapters claim it.
2. **Slow / Stalled Server**: Simulate a connection that hangs indefinitely.
   - *Expected Behavior*: Socket read timeout triggers, connection is aborted, and chunk is re-allocated.
3. **Corrupt Mirror**: Simulate a mirror returning invalid bytes for a range.
   - *Expected Behavior*: Chunk checksum fails, chunk is re-requested from a verified mirror, overall file SHA-256 remains valid.

---

## Superpower 4: Transparent User Alignment

- **Rule**: If a choice affects the user's workflow, system architecture, dependencies, or UI experience, **DO NOT DECIDE UNILATERALLY**.
- Always summarize:
  - The decision context
  - Option A vs Option B with trade-offs
  - Recommended path
  - Explicit question for the user
