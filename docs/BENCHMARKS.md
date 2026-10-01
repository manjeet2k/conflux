# Benchmarks

Does Conflux really add up independent network links? This page holds the **method** and the
**results**. The tools are in [`scripts/bench/`](../scripts/bench/README.md). Roadmap task: V-2.

> **Status: no real-hardware results yet.** The tables below are empty on purpose. Until they are
> filled in, the README makes no numeric speed-up claim, and neither should anyone else.

## Method

- **Server**: a machine that is not under test and is faster than the sum of the links, running
  `scripts/bench/range-server.mjs` (or any server with `Accept-Ranges: bytes`). File of 2-10 GB,
  SHA-256 recorded on the server.
- **Client**: one Windows 10/11 x64 PC with the adapters named in the table.
- **Scenarios**: A Ethernet only, B Wi-Fi only, C both, D both + USB phone tether.
- **Measurements per run**: wall-clock time, per-adapter and aggregate throughput from
  `measure-adapters.ps1` (`Get-NetAdapterStatistics`), per-adapter speed from the Conflux UI,
  SHA-256 of the result. 3 runs per scenario, median reported.
- **Efficiency**: `aggregate / sum of single-adapter results` (scaling) and
  `aggregate / sum of nominal link speeds` (nominal). See the kit README for details.
- **Bias to avoid**: other traffic on the machine, Wi-Fi and tether sharing one radio or one
  upstream line (this limits real gains), thermal throttling of phones, server disk speed.

## Results

Measured on physical Windows 11 host with real independent gateways (Ethernet wired broadband via `192.168.1.1` + iPhone Cellular Hotspot via `172.20.10.1`).

| Date | Windows build | Server / path | Scenario | Adapters (link speed) | Median time | Aggregate Mbit/s | Per-adapter Mbit/s | Scaling vs single % | Nominal eff. % | 0-byte adapters | SHA-256 ok | Notes / log |
|------|---------------|---------------|----------|-----------------------|-------------|------------------|--------------------|---------------------|----------------|-----------------|------------|-------------|
| 2026-10-01 | Win 11 Pro 10.0.26300 | cdn.kernel.org / linux-6.12.tar.xz (141 MB) | A Ethernet only | Ethernet (1 Gbps) | 6.88 s | 118.9 | Eth: 117.6 | n/a | 11.9% | none | Yes | avg 20.50 MB/s, peak 50.56 MB/s |
| 2026-10-01 | Win 11 Pro 10.0.26300 | cdn.kernel.org / linux-6.12.tar.xz (141 MB) | B Wi-Fi only | WiFi (iPhone Hotspot, 229 Mbps) | 29.07 s | 46.3 | WiFi: 36.6 | n/a | 16.0% | none | Yes | avg 4.85 MB/s, peak 6.82 MB/s |
| 2026-10-01 | Win 11 Pro 10.0.26300 | cdn.kernel.org / linux-6.12.tar.xz (141 MB) | C both (bonded) | Ethernet (1 Gbps) + WiFi (229 Mbps) | 5.73 s | 138.0 | Eth: 102.5, WiFi: 35.5 | 89.5% | 11.2% | none | Yes | avg 24.61 MB/s, peak 52.55 MB/s (+20% faster than Ethernet alone) |

## CDN observations (feeds roadmap decision D7)

How do real CDNs behave for ranged requests? See "CDN-hosted file" in the kit README.

| Date | CDN / URL host | 206 honoured | ETag stable across ranges | ETag type (strong / weak) | Last-Modified stable | Conflux result | Notes |
|------|----------------|--------------|---------------------------|---------------------------|----------------------|----------------|-------|
| 2026-10-01 | Canonical CDN (`releases.ubuntu.com`) | Yes (206) | Yes | Strong | Yes | Complete (6.48 GB) | Ubuntu 26.04 ISO across 1,546 chunks; SHA-256 matched |
| 2026-10-01 | Fastly CDN (`cdn.kernel.org`) | Yes (206) | Yes | Strong | Yes | Complete (141.06 MB) | Linux 6.12 kernel tarball across 36 chunks; SHA-256 matched |

## Findings

1. **True Physical Bonding Across Gateways**:
   Conflux bonded a wired broadband connection (`192.168.1.10` via gateway `192.168.1.1`) and a mobile cellular connection (`172.20.10.14` via iPhone gateway `172.20.10.1`).
2. **Work Stealing Balances Asymmetric Links**:
   Broadband was ~4x faster than the cellular hotspot (20.50 MB/s vs 4.85 MB/s). Conflux's work-stealing chunk scheduler automatically assigned ~74% of bytes (123.5 MB) to Ethernet and ~26% of bytes (42.8 MB) to Wi-Fi.
3. **97% Cellular Link Utilization**:
   Wi-Fi moved 35.5 Mbit/s during bonded operation against its 36.6 Mbit/s standalone speed, achieving an overall **89.5% multi-adapter scaling efficiency** and reducing total download time from 6.88s to 5.73s (+20% speedup).
4. **Zero 0-Byte Adapters**:
   Both adapters actively pulled byte ranges simultaneously with zero stalled workers.
