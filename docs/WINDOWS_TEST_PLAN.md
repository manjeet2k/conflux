# Windows test plan

Copy-pasteable PASS/FAIL checklists for verifying Conflux on real Windows 10/11 machines. All
Windows-only code is unverified until these have been run (roadmap tasks V-1, V-4, V-5).
Benchmarks are separate: see [BENCHMARKS.md](BENCHMARKS.md).

**How to use**: copy this file (or the relevant section) into an issue or a scratch file, fill
in the Result column with `PASS` or `FAIL` plus notes, and keep the evidence (screenshots, log
lines, hashes).

> **Every FAIL**: file a bug using the bug-report template (paste **Copy diagnostics**) **and**
> add a task for it to [ROADMAP.md](ROADMAP.md). A FAIL row without both is not finished.

## 0. Run header (fill in once per run)

| Field | Value |
|-------|-------|
| Tester / date | |
| Conflux version (installer or `cargo run`) | |
| Windows edition and build (`winver`) | |
| Physical or VM (VMs: note virtual NIC types) | |
| Adapters present (`Get-NetAdapter \| ft Name,InterfaceDescription,Status,LinkSpeed`) | |
| Test server URL (see section 5 for how to make one) | |
| Antivirus / EDR | |

Useful paths (PowerShell):

```powershell
$cfg  = "$env:APPDATA\com.conflux.desktop"            # settings, history (verify exact folders on your machine)
$logs = "$env:LOCALAPPDATA\com.conflux.desktop\logs"  # or use Settings > Open logs folder
```

---

## 1. Desktop unit tests (V-1 step 1)

Prerequisites: Rust toolchain from `rust-toolchain.toml`, Node 20+, MSVC build tools, repo checked out.

| # | Step | Expected | Result | Notes |
|---|------|----------|--------|-------|
| 1.1 | `cargo test -p conflux-desktop` | all tests pass, including settings migration, `set_override`, `partial_file_is_ours`, Zone.Identifier contents | PASS / FAIL | |
| 1.2 | `cargo test -p conflux-core` | all tests pass | PASS / FAIL | |
| 1.3 | Repeat 1.1 a second time | same result (no flaky tests) | PASS / FAIL | |

---

## 2. Smoke test on a running build (V-1 steps 2-7)

Build and launch from a developer checkout:

```powershell
npm --prefix ui ci
npm --prefix ui run build
cargo run -p conflux-desktop --release
```
(Or install a CI-built installer and start it from the Start menu.) Start the test server first
(section 5.1) and use a ~500 MB file so downloads last long enough to pause and kill.

| # | Step | Expected | Result | Notes |
|---|------|----------|--------|-------|
| 2.1 | Launch the app | Window appears with UI rendered (not a blank/white page, no CSP errors). Optional: right-click > Inspect (dev builds) and check the console for "Content Security Policy" violations | PASS / FAIL | |
| 2.2 | Look at the tray | Conflux icon is in the notification area; hover shows a tooltip; left-click shows the window; right-click shows a menu with Quit | PASS / FAIL | |
| 2.3 | Title bar buttons | Minimize, maximize/restore and close work; dragging the title bar moves the window; double-click behaviour is sane | PASS / FAIL | |
| 2.4 | Add a download (http URL of the test file) | Appears in the list, reaches Downloading with a speed, then Completed | PASS / FAIL | |
| 2.5 | Verify the file | `Get-FileHash <file> -Algorithm SHA256` equals the server's hash (`sha256sum` there) | PASS / FAIL | |
| 2.6 | Notification | A Windows toast appears on completion (Settings: notify on complete enabled; Focus Assist off) | PASS / FAIL | |
| 2.7 | Pause, then Resume a running download | Pauses (speed 0), resumes, finishes with the correct hash | PASS / FAIL | |
| 2.8 | Remove a download (once with, once without deleting the file, if offered) | Row disappears; file handling matches the dialog wording | PASS / FAIL | |
| 2.9 | Quit from the tray and relaunch | History still lists the earlier downloads | PASS / FAIL | |
| 2.10 | Mark of the Web: `Get-Content "<file>" -Stream Zone.Identifier` | Prints `[ZoneTransfer]`, `ZoneId=3`, and `HostUrl=` with the URL (no credentials) | PASS / FAIL | |
| 2.11 | Sparse flag: while a download is in progress, in another PowerShell `fsutil sparse queryflag "<partial file>"` | `This file is set as sparse` (record what you see once complete too) | PASS / FAIL | |
| 2.12 | Settings: change theme and chunk size, restart | Both persist | PASS / FAIL | |
| 2.13 | Network page: switch one adapter off, quit via tray, relaunch | Adapter is still off | PASS / FAIL | |
| 2.14 | Renew DHCP: `ipconfig /renew` (try to obtain a different IP: `ipconfig /release` then `/renew`, or change the router lease) | After the IP changes the adapter keeps its off/on choice (shown by name, not stale address) | PASS / FAIL | |
| 2.15 | Settings: `close_to_tray` ON, click the window close button | Window hides, app keeps running in the tray | PASS / FAIL | |
| 2.16 | Settings: `close_to_tray` OFF, click close | App exits (downloads are paused/saved; process gone from Task Manager) | PASS / FAIL | |
| 2.17 | Start 3 large downloads, tray > Quit | Exits within a few seconds (note seconds: ____), no leftover process; relaunch and resume all three to a correct hash | PASS / FAIL | |
| 2.18 | Launch Conflux a second time while the first runs (also while hidden in the tray) | No second window/process; the first window is brought to the front | PASS / FAIL | |
| 2.19 | Settings > Open logs folder | Explorer opens the log folder; a recent log file exists | PASS / FAIL | |
| 2.20 | Settings > Copy diagnostics, paste into Notepad | Shows version, OS, adapters with masked subnets (`x.x.x.x`-style host part), no folder paths, URLs or full IPs | PASS / FAIL | |
| 2.21 | Update check (if the build has the updater): trigger the check in Settings/About | Reports up to date or offers an update, with no error. Note what URL it contacts in the log | PASS / FAIL / N/A | |
| 2.22 | Add a URL that 404s, and one with a wrong host | Clear error message, no crash | PASS / FAIL | |
| 2.23 | Add a download with an existing file name in the folder | Does not overwrite; unique name chosen | PASS / FAIL | |

---

## 3. Clean-machine install, upgrade and uninstall (V-4)

Use a fresh VM (or a checkpoint-restored one) for each row group. Need: the installer from CI
(`Conflux_<ver>_x64-setup.exe`) and `SHA256SUMS.txt`, plus the previous beta's installer for the
upgrade test. Run the section on **Windows 10 (1809 or newer, ideally 22H2)** and **Windows 11**.

Record the OS in the column headers: W10 = ______ ; W11 = ______ .

| # | Step | Expected | W10 | W11 | Notes |
|---|------|----------|-----|-----|-------|
| 3.1 | Verify the download: `Get-FileHash .\Conflux_*_x64-setup.exe -Algorithm SHA256` vs `SHA256SUMS.txt` | Hashes match | | | |
| 3.2 | Double-click the installer (downloaded in a browser so it has Mark of the Web) | SmartScreen "Windows protected your PC" appears; More info > Run anyway proceeds. Record whether the publisher shows as "Unknown publisher" | | | |
| 3.3 | Install with WebView2 **already present** (default on Win 11 and updated Win 10) | No admin prompt (per-user); installs under `%LOCALAPPDATA%\...`; Start menu entry created | | | |
| 3.4 | Launch from the Start menu | UI renders; add and complete a small download | | | |
| 3.5 | Close; check Settings > Apps (Installed apps) | Conflux listed with the right version and a publisher string (record it) | | | |
| 3.6 | Upgrade: install previous beta, create settings + history + one completed download, then run the new installer over it | Upgrade completes without uninstalling first; settings and history preserved; app version updated | | | |
| 3.7 | In-app update (if updater exists): from the previous beta, accept an update | Downloads, installs, restarts into the new version; settings and history preserved | | | |
| 3.8 | Uninstall from Settings > Apps | App removed; Start menu entry and tray icon gone | | | |
| 3.9 | After uninstall: check your downloaded files | **Files in the download folder are untouched** (the hard requirement) | | | |
| 3.10 | After uninstall: check `%APPDATA%\com.conflux.desktop` | Record whether settings/history remain; this must match what the uninstaller says | | | |
| 3.11 | Reinstall after uninstall | Works; behaviour matches 3.10 (settings kept or fresh) | | | |
| 3.12 | **WebView2 absent**: use a Windows 10 VM image without WebView2 (or uninstall "Microsoft Edge WebView2 Runtime", noting Edge itself may keep it on some builds). Run the installer **with internet** | Installer downloads and installs the WebView2 bootstrapper, then the app starts | | | |
| 3.13 | WebView2 absent and **offline** (disconnect network before running) | Record exactly what happens: clear error message or silent failure. A silent failure is a FAIL | | | |
| 3.14 | **Offline installer** (if one is published: WebView2 embedded/fixed variant): install on an air-gapped VM | Installs and launches without any network access | PASS/FAIL/N/A | PASS/FAIL/N/A | |
| 3.15 | Install as a standard (non-admin) user | Works with no UAC prompt | | | |
| 3.16 | Antivirus: scan the installer and installed exe (Defender on, then with VirusTotal if you can) | Record any detection; false positives to be filed | | | |

---

## 4. Soak and chaos (V-5)

Goal: every run ends with a **correct file** or a **clear error**, and never with a corrupt file
reported as complete. Use a machine with two or more real adapters (Wi-Fi + Ethernet or tether).

Prepare (on the server; see section 5.1):

```bash
head -c 12G /dev/urandom > big12g.bin        # 10+ GB
sha256sum big12g.bin                          # record: SERVER_SHA = ________
node scripts/bench/range-server.mjs big12g.bin --port 8080
```

After every run compute the client hash and compare:

```powershell
Get-FileHash "D:\dl\big12g.bin" -Algorithm SHA256        # must equal SERVER_SHA (case-insensitive)
```

| # | Scenario and steps | Expected | Client SHA-256 (or error shown) | Match | Result |
|---|--------------------|----------|---------------------------------|-------|--------|
| 4.1 | **Baseline**: download the 12 GB file with all adapters on | Completes; hash matches; per-adapter speeds shown | | Y / N | PASS / FAIL |
| 4.2 | **Wi-Fi off/on mid-download**: at about 30%, disable Wi-Fi (`Disable-NetAdapter -Name "Wi-Fi" -Confirm:$false`); wait 60 s; `Enable-NetAdapter -Name "Wi-Fi"` | Download continues on the other adapter(s); Wi-Fi rejoins after re-enable (hot-join); completes; hash matches | | Y / N | PASS / FAIL |
| 4.3 | **Unplug**: remove the USB tether or Ethernet cable mid-download, replug after 60 s | Same as 4.2 | | Y / N | PASS / FAIL |
| 4.4 | **Only adapter lost**: during a single-adapter download disable that adapter for 2 minutes, then re-enable | Download stalls then recovers (or fails with a clear error that can be resumed); never reports complete with a bad hash | | Y / N | PASS / FAIL |
| 4.5 | **Sleep/resume**: at about 40%, put the laptop to sleep (`rundll32 powrprof.dll,SetSuspendState 0,1,0` or the Start menu) for 5 minutes, wake it | Download recovers or can be resumed; completes; hash matches | | Y / N | PASS / FAIL |
| 4.6 | **Kill and resume**: at about 50%, `Stop-Process -Name conflux-desktop -Force` (use the real process name from Task Manager); confirm the `<file>.conflux.json` sidecar exists; relaunch and resume | Resumes from the sidecar (does not restart from 0; note the percentage resumed at: ____); completes; hash matches | | Y / N | PASS / FAIL |
| 4.7 | **Kill the server mid-download** (Ctrl+C the Node server), wait 30 s, restart it | Download retries and completes, or fails with a clear message and can be resumed; hash matches | | Y / N | PASS / FAIL |
| 4.8 | **Disk full**: use a small volume (a 2 GB VHD mounted as a drive, `diskpart` / Disk Management) as the save folder and download the 12 GB file | Fails with a clear "not enough disk space" style error, ideally before downloading; no crash, no partial file reported as complete. Also test: fill the disk **during** a download by copying a big file in | | n/a | PASS / FAIL |
| 4.9 | **Revoked folder**: start a download into `D:\dl\locked`, then deny write access (`icacls D:\dl\locked /deny "$env:USERNAME:(OI)(CI)W"`) or delete/rename the folder; later restore it (`icacls D:\dl\locked /remove:d $env:USERNAME`) | Clear error naming the folder problem; download can be resumed after access is restored; no corrupt completion | | Y / N | PASS / FAIL |
| 4.10 | **Concurrent soak**: add 10 downloads at once (mix of 500 MB-2 GB files on the test server) and leave running for **1 hour**; watch memory/CPU in Task Manager at 0, 30 and 60 min | All complete, all hashes match; the UI stays responsive; memory does not grow without bound (record MB at each point: ____ / ____ / ____) | (table below) | Y / N | PASS / FAIL |
| 4.11 | **Range-less server**: `node range-server.mjs file --no-ranges` | Single-stream download completes with correct hash, with no errors about ranges | | Y / N | PASS / FAIL |
| 4.12 | **Server file changes mid-download** (replace the file on the server at 30%; ETag changes) | Download detects the change and restarts or fails clearly; never produces a mixed-content file reported OK | | Y / N | PASS / FAIL |

Hashes for 4.10 (one row per file):

| File | Server SHA-256 | Client SHA-256 | Match |
|------|----------------|----------------|-------|
| | | | |

If the log has anything alarming, keep it: set `RUST_LOG=conflux_core=debug` in the shell before
launching the app from that shell and attach the log from the Open logs folder location.

---

## 5. Appendix

### 5.1 Test server

On any other PC or VPS with Node 18+ (script: [`scripts/bench/range-server.mjs`](../scripts/bench/range-server.mjs),
details in [`scripts/bench/README.md`](../scripts/bench/README.md)):

```
node range-server.mjs <file-or-folder> --port 8080 [--throttle-mbps 40] [--no-ranges] [--no-etag]
```

The server logs each request with the peer IP, so you can confirm which adapter carried which
byte range.

### 5.2 Summary table (fill at the end)

| Section | Rows | PASS | FAIL | N/A | Bugs filed | Roadmap tasks added |
|---------|------|------|------|-----|------------|---------------------|
| 1 Desktop tests | 3 | | | | | |
| 2 Smoke | 23 | | | | | |
| 3 Install | 16 | | | | | |
| 4 Soak/chaos | 12 | | | | | |

Reminder: every FAIL needs a filed bug **and** a roadmap task. When a section is complete, add
its date and result to the *Done notes* of roadmap tasks V-1, V-4 and V-5 (what was **not**
verified, too).
