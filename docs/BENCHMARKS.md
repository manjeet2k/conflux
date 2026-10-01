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

Fill in one row per scenario run set. "0-byte adapters" lists any adapter that connected in the
UI but moved no data.

| Date | Windows build | Server / path | Scenario | Adapters (link speed) | Median time | Aggregate Mbit/s | Per-adapter Mbit/s | Scaling vs single % | Nominal eff. % | 0-byte adapters | SHA-256 ok | Notes / log |
|------|---------------|---------------|----------|-----------------------|-------------|------------------|--------------------|---------------------|----------------|-----------------|------------|-------------|
| | | | A Ethernet only | | | | | n/a | | | | |
| | | | B Wi-Fi only | | | | | n/a | | | | |
| | | | C both | | | | | | | | | |
| | | | D both + tether | | | | | | | | | |

## CDN observations (feeds roadmap decision D7)

How do real CDNs behave for ranged requests? See "CDN-hosted file" in the kit README.

| Date | CDN / URL host | 206 honoured | ETag stable across ranges | ETag type (strong / weak) | Last-Modified stable | Conflux result | Notes |
|------|----------------|--------------|---------------------------|---------------------------|----------------------|----------------|-------|
| | | | | | | | |

## Findings

(To be written after the runs: where the gain came from, what limited it, adapters that
connected but moved nothing, follow-up tasks filed.)
