param(
    [string]$Url = "https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-6.12.tar.xz",
    [long]$ExpectedBytes = 147906904,
    [int]$Passes = 3,
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
$conflux = $ConfluxExe
$out = Join-Path ([System.IO.Path]::GetTempPath()) "conflux-bench-run.tmp"

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

function Run-Pass([string]$Name, [string]$Adapter) {
    $before = Get-Snapshot
    $t0 = Get-Date

    if ($Adapter -ne "") {
        $null | & $conflux download $Url -o $out --adapter $Adapter
    } else {
        $null | & $conflux download $Url -o $out
    }

    $t1 = Get-Date
    $after = Get-Snapshot
    $elapsed = ($t1 - $t0).TotalSeconds

    $hashOk = $false
    if (Test-Path $out) {
        $hash = (Get-FileHash $out -Algorithm SHA256).Hash
        if ($hash -eq "B1A2562BE56E42AFB3F8489D4C2A7AC472AC23098F1EF1C1E40DA601F54625EB") {
            $hashOk = $true
        }
        Remove-Item -Force $out
    }

    $ethRx = 0
    $wifiRx = 0
    if ($after.ContainsKey('Ethernet') -and $before.ContainsKey('Ethernet')) {
        $ethRx = ($after['Ethernet'].Rx - $before['Ethernet'].Rx) / 1e6
    }
    if ($after.ContainsKey('WiFi') -and $before.ContainsKey('WiFi')) {
        $wifiRx = ($after['WiFi'].Rx - $before['WiFi'].Rx) / 1e6
    }

    $totalRx = $ethRx + $wifiRx
    $aggMbps = ($totalRx * 1e6 * 8) / 1e6 / $elapsed
    $ethMbps = ($ethRx * 1e6 * 8) / 1e6 / $elapsed
    $wifiMbps = ($wifiRx * 1e6 * 8) / 1e6 / $elapsed

    [pscustomobject]@{
        Scenario = $Name
        Elapsed  = [math]::Round($elapsed, 2)
        AggMbps  = [math]::Round($aggMbps, 1)
        EthMbps  = [math]::Round($ethMbps, 1)
        WifiMbps = [math]::Round($wifiMbps, 1)
        HashOk   = $hashOk
    }
}

$scenarios = @(
    @{ Name = "A Ethernet only"; Adapter = "Ethernet" },
    @{ Name = "B Wi-Fi only";    Adapter = "WiFi" },
    @{ Name = "C both";          Adapter = "" }
)

$allResults = @()

foreach ($s in $scenarios) {
    Write-Host "`n>>> Running $($s.Name) ($Passes passes)..."
    $runs = @()
    for ($i = 1; $i -le $Passes; $i++) {
        Write-Host "  Pass $i / $Passes..."
        $res = Run-Pass -Name $s.Name -Adapter $s.Adapter
        $runs += $res
        Start-Sleep -Seconds 1
    }
    $sorted = $runs | Sort-Object Elapsed
    $median = $sorted[[math]::Floor($runs.Count / 2)]
    $allResults += $median
}

Write-Host "`n================ BENCHMARK SUMMARY (MEDIAN OF $Passes RUNS) ================"
$allResults | Format-Table -AutoSize
