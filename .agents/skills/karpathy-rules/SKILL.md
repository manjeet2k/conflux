---
name: karpathy-rules
description: >-
  Use this skill to guide software architecture, development, and debugging using Andrej Karpathy's
  first-principles engineering methodology: establishing minimal working baselines first, inspecting raw
  data and byte streams, avoiding cargo-culting, keeping mental models simple, and following "Make It Work,
  Make It Right, Make It Fast".
---

# Karpathy Engineering Rules

This skill enforces Andrej Karpathy's engineering philosophy throughout the design, implementation, and debugging of systems software.

---

## Core Protocols

### 1. First Principles Check
Before writing any code or introducing a dependency, answer:
- *What is the underlying physical/OS mechanism?*
  (e.g., How does the OS kernel route an outbound packet when bound to `INADDR_ANY` vs a specific local IP address?)
- *What does the minimal implementation require?*
- *Are we assuming something without verifying it?*

### 2. The Minimal Baseline Protocol
- **Never jump straight to complex architectures.**
- Build the simplest possible end-to-end slice that validates the core premise:
  1. Download a 10MB test file with 2 chunks sequentially.
  2. Compute SHA-256 and assert exact match against the source.
  3. Only then introduce concurrent workers.
  4. Only then introduce multi-adapter binding.
  5. Only then introduce the GUI.

### 3. "Inspect the Data" Protocol
- When building or debugging data-intensive software (like Conflux):
  - Print the exact byte ranges: `[start, end, size]`.
  - Print the local bound socket address: `local_addr = socket.local_addr()`.
  - Print the remote socket address: `peer_addr = socket.peer_addr()`.
  - Verify headers: Check for `Content-Range: bytes 0-1048575/10485760`.
  - If a test fails, inspect the first 64 bytes of mismatched data in hex.

### 4. Progression: Work → Right → Fast
1. **Make It Work**: Get the correct output. Focus on byte-level integrity.
2. **Make It Right**: Clean up boundaries, handle errors, edge cases, and connection drops.
3. **Make It Fast**: Benchmark before optimizing. Eliminate unnecessary allocations and lock contention.

### 5. Simplicity & Clarity
- If a junior developer cannot trace the flow of execution from socket creation to disk write within 5 minutes, simplify the architecture.
- Prefer explicit state machines and loops over deeply nested async macros or opaque abstraction layers.
