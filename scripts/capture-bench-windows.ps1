<#
.SYNOPSIS
  Runs capture-bench (synthetic test-pattern window only) and samples its CPU, working
  set, handles and threads from outside every -SampleSec seconds (CAP-02A CAP-PERF-*).

.EXAMPLE
  powershell -NoProfile -File scripts\capture-bench-windows.ps1 -Label bench-720p -BenchArgs "--seconds 60 --preview 1280x720"
#>
param(
  [string]$Exe = "target\release\capture-bench.exe",
  [string]$Label = "bench",
  [string]$BenchArgs = "--seconds 30",
  [string]$EvidenceDir = "docs\qa\evidence\CAP-02A\perf",
  [int]$SampleSec = 2,
  [switch]$Gpu
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version 3
$repo = Split-Path -Parent $PSScriptRoot
$exePath = (Resolve-Path (Join-Path $repo $Exe)).Path
$outDir = Join-Path $repo $EvidenceDir
New-Item -ItemType Directory -Force $outDir | Out-Null
$base = Join-Path $outDir $Label

$bench = Start-Process -FilePath $exePath -ArgumentList $BenchArgs -PassThru -NoNewWindow `
  -RedirectStandardOutput "$base.jsonl" -RedirectStandardError "$base.stderr.log"
# Reading the handle now keeps ExitCode available after the process exits.
$null = $bench.Handle
"t_s,cpu_pct,ws_mb,private_mb,handles,threads,gpu_max_engine_pct,gpu_3d_pct,gpu_copy_pct" | Out-File -Encoding utf8 "$base.process.csv"
$started = Get-Date
$prevCpu = $null; $prevTime = $null
while (-not $bench.HasExited) {
  try {
    $p = Get-Process -Id $bench.Id -ErrorAction Stop
    $now = Get-Date
    $cpu = ""
    if ($null -ne $prevTime) {
      $cpu = [math]::Round(100.0 * ($p.TotalProcessorTime.TotalMilliseconds - $prevCpu) /
        (($now - $prevTime).TotalMilliseconds * [Environment]::ProcessorCount), 2)
    }
    $prevCpu = $p.TotalProcessorTime.TotalMilliseconds; $prevTime = $now
    $gMax = ""; $g3d = ""; $gCopy = ""
    if ($Gpu) {
      # GPU engine utilisation of this process (Windows performance counters; no install).
      $counter = Get-Counter -Counter "\GPU Engine(pid_$($bench.Id)_*)\Utilization Percentage" -ErrorAction SilentlyContinue
      $samples = if ($null -ne $counter) { @($counter.CounterSamples) } else { @() }
      if ($samples.Count -gt 0) {
        $gMax = [math]::Round(($samples | Measure-Object CookedValue -Maximum).Maximum, 2)
        $g3d = [math]::Round(($samples | Where-Object { $_.InstanceName -match 'engtype_3d' } | Measure-Object CookedValue -Sum).Sum, 2)
        $gCopy = [math]::Round(($samples | Where-Object { $_.InstanceName -match 'engtype_copy' } | Measure-Object CookedValue -Sum).Sum, 2)
      }
    }
    (@([int]($now - $started).TotalSeconds, $cpu, [math]::Round($p.WorkingSet64 / 1MB, 1),
      [math]::Round($p.PrivateMemorySize64 / 1MB, 1), $p.HandleCount, $p.Threads.Count, $gMax, $g3d, $gCopy) -join ",") |
      Out-File -Append -Encoding utf8 "$base.process.csv"
  } catch { break }
  Start-Sleep -Seconds $SampleSec
}
$bench.WaitForExit()
"exit=$($bench.ExitCode) cpuCount=$([Environment]::ProcessorCount) args=$BenchArgs" | Out-File -Encoding utf8 "$base.exit.txt"
"bench $Label exit=$($bench.ExitCode)"
