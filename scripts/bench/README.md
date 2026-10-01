# Conflux benchmark kit

Tools for measuring whether Conflux really aggregates bandwidth across physical adapters.
Methodology and the results table live in [docs/BENCHMARKS.md](../../docs/BENCHMARKS.md).

| File | What it is |
|------|------------|
| `range-server.mjs` | Dependency-free Node (18+) static file server with `Range` support, optional per-connection throttle and a request log that shows the **peer IP** of every connection |
| `measure-adapters.ps1` | PowerShell script that snapshots `Get-NetAdapterStatistics` before and after a download and prints throughput per adapter and in aggregate |

The Node server was tested on Linux with curl (206/416 responses, byte-exact ranges, throttling,
peer logging). **The PowerShell script has not been run** (no PowerShell on the dev host); it is
short on purpose. If it misbehaves, fix it and note that in `docs/BENCHMARKS.md`.

## 1. Set up the server

Pick a machine that is NOT the one under test (a second PC or a Linux VPS) and that is faster
than the sum of your links. Make a test file (2-10 GB is enough; the transfer must last at least
30 s so the scheduler can settle):

```powershell
fsutil file createnew C:\bench\test.bin 4294967296        # Windows (zero-filled, 4 GiB)
```
```bash
head -c 4G /dev/urandom > test.bin                        # Linux
sha256sum test.bin                                        # note it; compare after each run
```

Run the server (Windows firewall will ask to allow Node on private networks):

```
node range-server.mjs test.bin --port 8080
node range-server.mjs test.bin --port 8080 --throttle-mbps 40   # cap EACH connection at 40 Mbit/s
```

Options: `--host`, `--no-etag` (omit ETag/Last-Modified), `--no-ranges` (single-stream fallback).
Each request prints `peer=<ip>:<port> ... range=bytes=a-b -> 206 sent=N`. The peer IP is the
address of the local adapter the client bound to, so you can see which adapter carried which
chunk. The server must be reachable from every adapter under test (e.g. a server on the
router's LAN reachable from both Ethernet and Wi-Fi; for the tether case use a public VPS).

Throttling note: the cap is per connection, so with N connections per adapter the cap is
N times larger per adapter. Use it to imitate slow links on a fast LAN, and report it.

## 2. Scenarios

Run each at least 3 times and record the median. Between runs, delete the partial file and its
`.conflux.json` sidecar. Close other network-heavy programs (the counters include all traffic).

| # | Scenario | In Conflux | Expected |
|---|----------|------------|----------|
| A | Ethernet only | Enable only the Ethernet adapter on the Network page | about the Ethernet link speed |
| B | Wi-Fi only | Enable only Wi-Fi | about the Wi-Fi link speed |
| C | Both | Enable Ethernet and Wi-Fi | at most A + B |
| D | Both + phone tether | Also plug in a USB tether (RNDIS/NCM) and enable it | at most A + B + tether |

For each run:

1. Open PowerShell in this folder. Start the sampler **before** you press Start in Conflux:
   `powershell -ExecutionPolicy Bypass -File .\measure-adapters.ps1 -ExpectedBytes 4294967296 -LinkMbps @{ 'Ethernet' = 1000; 'Wi-Fi' = 300 }`
   (adapter names as shown by `Get-NetAdapter`; link speeds are your nominal `LinkSpeed` values).
2. Add the URL (`http://<server-ip>:8080/test.bin`) in Conflux and start it.
3. Press Enter in the sampler the moment the download completes.
4. Record from the sampler: per-adapter MB and Mbit/s, aggregate Mbit/s. Record from Conflux:
   the per-adapter speeds shown in the UI and the total time.
5. Verify integrity: `Get-FileHash <file> -Algorithm SHA256` equals the server-side hash.
6. Optionally repeat with `$env:RUST_LOG='conflux_core=debug'` set before launching the app from
   a terminal, and keep the log with the results.

## 3. Reading efficiency

```
efficiency % = aggregate observed Mbit/s / (sum of the nominal link speeds of the adapters used) * 100
```

Nominal link speed over-states what a link delivers (Wi-Fi especially), so the better figure is
**scaling vs. single-adapter runs**: `C / (A + B) * 100` and `D / (A + B + tether) * 100`. Both
numbers go in the table. Values near 100 % mean the scheduler is not the bottleneck; low values
mean the server, a shared bottleneck upstream (both links ending in the same ISP line), or a
stalled adapter. Always check the sampler's list of adapters that moved ~0 bytes: an adapter
that Conflux listed but that carried nothing is a finding (see KNOWN_ISSUES: no-gateway adapters).

## 4. CDN-hosted file: ETag / Last-Modified on 206 responses (feeds decision D7)

Conflux validates that every ranged response (206) belongs to the same file revision. Some
CDNs answer different range requests from different edge nodes with different ETags. To observe
this without Conflux, from PowerShell or Git Bash (use a large public file on the CDN you care
about, e.g. a release asset or a vendor ISO):

```
curl.exe -s -D - -o NUL -r 0-1023 https://cdn.example.com/big.iso
curl.exe -s -D - -o NUL -r 5000000-5001023 https://cdn.example.com/big.iso
curl.exe -s -D - -o NUL -r 0-1023 https://cdn.example.com/big.iso     # repeat a few times
```

Record for each response: status (206 or 200), `ETag` (strong or weak `W/`), `Last-Modified`,
`Content-Range`, `Accept-Ranges`, and any `Server`/`Via`/`X-Cache` hint. Then download the same
URL with Conflux and note whether it completes or fails with a validator mismatch, and with how
many connections. Report: CDN name, whether ETags were stable across ranges, and the outcome.
Put the observations in the "CDN observations" table in `docs/BENCHMARKS.md`.
