<#
.SYNOPSIS
  Records per-adapter byte counters around a download and prints throughput per adapter
  plus the aggregate. Deliberately simple; it has NOT been run by its author (no PowerShell
  on the dev host), so treat the first run as a test of the script itself.

.DESCRIPTION
  Usage:
    1. Start the download in Conflux, but do not click anything yet if you can avoid it.
    2. powershell -ExecutionPolicy Bypass -File .\measure-adapters.ps1
    3. The script snapshots Get-NetAdapterStatistics, then waits for you to press Enter
       when the download has FINISHED (or use -Seconds N to sample a fixed window), then
       snapshots again and prints the difference.

  Better: run it BEFORE pressing "Start" in Conflux, press Enter in the script as soon as
  the download completes, and give the same wall-clock seconds to the app's own figure.

.PARAMETER Seconds
  Sample for a fixed number of seconds instead of waiting for Enter.
.PARAMETER ExpectedBytes
  Size of the file being downloaded. If given, prints how much of the received traffic is
  explained by the file (protocol overhead shows up as the ratio).
.PARAMETER LinkMbps
  Optional hashtable of nominal link speeds by adapter name, e.g.
  -LinkMbps @{ 'Ethernet' = 1000; 'Wi-Fi' = 300 }. Used to print efficiency.
#>
param(
    [int]$Seconds = 0,
    [long]$ExpectedBytes = 0,
    [hashtable]$LinkMbps = @{}
)

function Get-Snapshot {
    $snap = @{}
    Get-NetAdapterStatistics | ForEach-Object {
        $snap[$_.Name] = [pscustomobject]@{
            Name = $_.Name
            Rx   = [int64]$_.ReceivedBytes
            Tx   = [int64]$_.SentBytes
        }
    }
    return $snap
}

$before = Get-Snapshot
$t0 = Get-Date
Write-Host "Snapshot taken at $($t0.ToString('o')) for $($before.Count) adapters."

if ($Seconds -gt 0) {
    Write-Host "Sampling for $Seconds seconds..."
    Start-Sleep -Seconds $Seconds
} else {
    Read-Host "Press Enter the moment the download finishes"
}

$after = Get-Snapshot
$elapsed = ((Get-Date) - $t0).TotalSeconds
if ($elapsed -le 0) { $elapsed = 1 }

$rows = foreach ($name in $after.Keys) {
    if (-not $before.ContainsKey($name)) { continue }
    $rx = $after[$name].Rx - $before[$name].Rx
    $mbps = ($rx * 8) / 1e6 / $elapsed
    $row = [ordered]@{
        Adapter    = $name
        RxMB       = [math]::Round($rx / 1e6, 2)
        RxMbps     = [math]::Round($mbps, 1)
    }
    if ($LinkMbps.ContainsKey($name) -and $LinkMbps[$name] -gt 0) {
        $row.LinkMbps = $LinkMbps[$name]
        $row.UtilPct  = [math]::Round(100 * $mbps / $LinkMbps[$name], 1)
    }
    [pscustomobject]$row
}

# Hide idle adapters (< 0.1 MB) so the table is readable, but say how many were hidden.
$busy = @($rows | Where-Object { $_.RxMB -ge 0.1 })
$idle = @($rows | Where-Object { $_.RxMB -lt 0.1 })
$busy | Sort-Object RxMbps -Descending | Format-Table -AutoSize

$totalRx = ($rows | Measure-Object -Property RxMB -Sum).Sum
$aggMbps = [math]::Round(($totalRx * 1e6 * 8) / 1e6 / $elapsed, 1)
Write-Host ("Elapsed: {0:N1} s   Aggregate received: {1:N2} MB   Aggregate: {2} Mbit/s" -f $elapsed, $totalRx, $aggMbps)
if ($idle.Count -gt 0) {
    Write-Host ("Adapters with ~0 bytes received (note any that Conflux said it was using!): " + (($idle | ForEach-Object { $_.Adapter }) -join ', '))
}
if ($ExpectedBytes -gt 0) {
    $ratio = [math]::Round(100 * ($totalRx * 1e6) / $ExpectedBytes, 1)
    Write-Host "Received bytes / file size = $ratio % (100-105 % is normal; much higher means retries or other traffic on the machine)."
}
Write-Host "Note: counters include ALL traffic on the adapter, so close browsers/updaters during a run."
