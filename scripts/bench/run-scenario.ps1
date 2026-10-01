param(
    [string]$ScenarioName = "Ethernet only",
    [string]$Url = "https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-6.12.tar.xz",
    [string]$OutputFile = "",
    [long]$ExpectedBytes = 147906904,
    [string]$AdapterFilter = "",
    [string]$ConfluxExe = ""
)

if (-not $ConfluxExe) {
    $candidate = Join-Path $PSScriptRoot "..\..\target\x86_64-pc-windows-gnu\debug\conflux.exe"
    if (Test-Path $candidate) {
        $ConfluxExe = [System.IO.Path]::GetFullPath($candidate)
    } else {
        $ConfluxExe = "conflux.exe"
    }
}

if (-not $OutputFile) {
    $OutputFile = Join-Path ([System.IO.Path]::GetTempPath()) "conflux-bench-test.tmp"
}

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

Write-Host "=== Starting Conflux Download for [$ScenarioName] ==="
if ($AdapterFilter -ne "") {
    $null | & $ConfluxExe download $Url -o $OutputFile --adapter $AdapterFilter
} else {
    $null | & $ConfluxExe download $Url -o $OutputFile
}

$after = Get-Snapshot
$t1 = Get-Date
$elapsed = ($t1 - $t0).TotalSeconds
if ($elapsed -le 0) { $elapsed = 1 }

$hash = (Get-FileHash $OutputFile -Algorithm SHA256).Hash
Remove-Item -Force $OutputFile

Write-Host "`n=== Benchmark Results for [$ScenarioName] ==="
Write-Host ("Wall-clock Elapsed: {0:N2} s" -f $elapsed)
Write-Host "Output File SHA-256: $hash"

$rows = foreach ($name in $after.Keys) {
    if (-not $before.ContainsKey($name)) { continue }
    $rx = $after[$name].Rx - $before[$name].Rx
    $mbps = ($rx * 8) / 1e6 / $elapsed
    [pscustomobject]@{
        Adapter = $name
        RxMB    = [math]::Round($rx / 1e6, 2)
        RxMbps  = [math]::Round($mbps, 1)
    }
}

$busy = @($rows | Where-Object { $_.RxMB -ge 0.1 })
$busy | Sort-Object RxMbps -Descending | Format-Table -AutoSize

$totalRx = ($rows | Measure-Object -Property RxMB -Sum).Sum
$aggMbps = [math]::Round(($totalRx * 1e6 * 8) / 1e6 / $elapsed, 1)
Write-Host ("Aggregate throughput: {0} Mbit/s ({1:N2} MB in {2:N2} s)" -f $aggMbps, $totalRx, $elapsed)
