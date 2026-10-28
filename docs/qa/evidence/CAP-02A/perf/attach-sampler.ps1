<#
  CAP-02A: samples an already running process (soak run 2), because the wrapper's first
  sample hit a GPU-counter bug and its sampling loop ended (fixed in
  scripts/capture-bench-windows.ps1). Same columns; t_s is measured from the process start.
    powershell -NoProfile -File docs\qa\evidence\CAP-02A\perf\attach-sampler.ps1 -ProcessId <pid> -Csv <file>
#>
param([int]$ProcessId, [string]$Csv, [int]$SampleSec = 10)
$ErrorActionPreference = "Stop"
Set-StrictMode -Version 3
$proc = Get-Process -Id $ProcessId
$origin = $proc.StartTime
$prevCpu = $null; $prevTime = $null
while ($true) {
  try { $p = Get-Process -Id $ProcessId -ErrorAction Stop } catch { break }
  $now = Get-Date
  $cpu = ""
  if ($null -ne $prevTime) {
    $cpu = [math]::Round(100.0 * ($p.TotalProcessorTime.TotalMilliseconds - $prevCpu) /
      (($now - $prevTime).TotalMilliseconds * [Environment]::ProcessorCount), 2)
  }
  $prevCpu = $p.TotalProcessorTime.TotalMilliseconds; $prevTime = $now
  $gMax = ""; $g3d = ""; $gCopy = ""
  $counter = Get-Counter -Counter "\GPU Engine(pid_$($ProcessId)_*)\Utilization Percentage" -ErrorAction SilentlyContinue
  $samples = if ($null -ne $counter) { @($counter.CounterSamples) } else { @() }
  if ($samples.Count -gt 0) {
    $gMax = [math]::Round(($samples | Measure-Object CookedValue -Maximum).Maximum, 2)
    $g3d = [math]::Round(($samples | Where-Object { $_.InstanceName -match 'engtype_3d' } | Measure-Object CookedValue -Sum).Sum, 2)
    $gCopy = [math]::Round(($samples | Where-Object { $_.InstanceName -match 'engtype_copy' } | Measure-Object CookedValue -Sum).Sum, 2)
  }
  (@([int]($now - $origin).TotalSeconds, $cpu, [math]::Round($p.WorkingSet64 / 1MB, 1),
    [math]::Round($p.PrivateMemorySize64 / 1MB, 1), $p.HandleCount, $p.Threads.Count, $gMax, $g3d, $gCopy) -join ",") |
    Out-File -Append -Encoding utf8 $Csv
  Start-Sleep -Seconds $SampleSec
}
"attached sampler finished for pid $ProcessId"
