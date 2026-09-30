---
name: conflux-dev
description: >-
  Use this skill for Conflux-specific engineering workflows: testing local network interface discovery,
  validating low-level socket binding to specific adapter IPs, sparse file pre-allocation, running automated
  verification suites, and cross-compiling for Windows from Linux/WSL2.
---

# Conflux Development & Verification Runbook

This skill provides step-by-step procedural workflows for developing, testing, and cross-compiling Conflux.

---

## 1. Network Adapter Verification

### Physical Adapter Discovery
- On Linux / WSL2: Inspect `ip -br a` or query via Rust crate (`getifaddrs` / `pnet_datalink`).
- On Windows: WinSock `GetAdaptersAddresses` API via `windows-sys` or `ipconfig /all`.
- Verification checklist:
  - Loopback (`127.0.0.1` / `::1`) is identified and separated from physical adapters.
  - Virtual bridges / WSL virtual switches are flagged.
  - Active IPv4 addresses with an active default gateway are selected.

---

## 2. Low-Level Socket Binding Protocol

To force the OS kernel to route outgoing traffic through a specific physical adapter:
```rust
use socket2::{Socket, Domain, Type, Protocol};
use std::net::SocketAddr;

// 1. Create a raw TCP socket
let socket = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP))?;

// 2. Bind directly to the local adapter IP (port 0 = OS picks ephemeral port)
let local_addr: SocketAddr = format!("{adapter_ip}:0").parse()?;
socket.bind(&local_addr.into())?;

// 3. Connect to the remote server
let remote_addr: SocketAddr = resolve_remote_addr(server_host, server_port)?;
socket.connect(&remote_addr.into())?;
```

---

## 3. Sparse File Writing & Pre-allocation

To prevent disk fragmentation and achieve zero-copy downloads:
```rust
use tokio::fs::File;
use tokio::io::AsyncSeekExt;

// 1. Create file and pre-allocate full file length instantly
let mut file = File::create(&save_path).await?;
file.set_len(total_bytes).await?; // Truncate/pre-allocate

// 2. Concurrently write chunks at exact byte offsets
// Using tokio seek or direct Windows WriteFileAt / pwrite
```

---

## 4. Windows Cross-Compilation Check

From Linux/WSL2, verify that code compiles cleanly for Windows:
```bash
cargo check --target x86_64-pc-windows-gnu
```

To build a release Windows `.exe`:
```bash
cargo build --target x86_64-pc-windows-gnu --release
```
The resulting executable is located at `target/x86_64-pc-windows-gnu/release/conflux.exe`.
