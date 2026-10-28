<#
.SYNOPSIS
  CAP-02A real-device QA session for the local self-preview (T-CAP-RUN-*, CAP-PERF-*).

.DESCRIPTION
  Starts the synthetic test-pattern window (topmost: WGC delivered no frames for it while
  it was covered, a CAP-02A finding) and one release tp-desktop.exe, then samples
  the app's process tree every -SampleSec seconds (CPU, working set, handles, threads,
  GPU engine utilisation, network endpoints) into a CSV while a person operates the app.
  A person must choose the capture target in the Windows picker; this script never
  automates the picker or synthesises input.

  Control files (created by the operator / QA lead, next to the evidence):
    <label>.shot  -> PrintWindow screenshot of the TP Remote window only (never the
                     screen). Take one only while the preview shows the test pattern.
    <label>.resize -> resizes the test-pattern window once (T-CAP-WIN-08 at app level).
    <label>.closepattern -> closes the test-pattern window (T-CAP-WIN-09 at app level).
    <label>.stop  -> ends the session: WM_CLOSE to the app (during capture if active).

  Only processes started by this script are ever stopped.

.EXAMPLE
  powershell -NoProfile -File scripts\capture-qa-windows.ps1 -Label session1 -DurationSec 3600
#>
param(
  [string]$Exe = "target\release\tp-desktop.exe",
  [string]$PatternExe = "target\release\tp-test-pattern.exe",
  [string]$Label = "session",
  [string]$EvidenceDir = "docs\qa\evidence\CAP-02A\app",
  [int]$DurationSec = 600,
  [int]$SampleSec = 5,
  [int]$PatternX = -1700,
  [int]$PatternY = 200,
  [int]$PatternW = 1280,
  [int]$PatternH = 760,
  [switch]$NoPattern,
  [switch]$NoGpu,
  [int]$ExitTimeoutSec = 15
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version 3

$repo = Split-Path -Parent $PSScriptRoot
$exePath = (Resolve-Path (Join-Path $repo $Exe)).Path
$outDir = Join-Path $repo $EvidenceDir
New-Item -ItemType Directory -Force $outDir | Out-Null
$base = Join-Path $outDir $Label

Add-Type -ReferencedAssemblies System.Drawing -TypeDefinition @"
using System;
using System.Drawing;
using System.Drawing.Imaging;
using System.Runtime.InteropServices;

public static class TpQa {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint flags);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int cx, int cy, uint flags);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("dwmapi.dll")] public static extern int DwmFlush();

  public static string Capture(IntPtr h, string path) {
    RECT r; GetWindowRect(h, out r);
    int w = r.Right - r.Left, hh = r.Bottom - r.Top;
    using (Bitmap bmp = new Bitmap(w, hh, PixelFormat.Format32bppArgb)) {
      using (Graphics g = Graphics.FromImage(bmp)) {
        IntPtr hdc = g.GetHdc();
        bool ok = PrintWindow(h, hdc, 2);
        g.ReleaseHdc(hdc);
        if (!ok) { return "PrintWindow failed"; }
      }
      bmp.Save(path, ImageFormat.Png);
    }
    return w + "x" + hh;
  }
}
"@

function Get-Tree([int]$rootPid) {
  $all = @(Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId, Name)
  $tree = @($all | Where-Object { $_.ProcessId -eq $rootPid })
  $seen = @($rootPid)
  $frontier = @($rootPid)
  while ($frontier.Count -gt 0) {
    $children = @($all | Where-Object { ($frontier -contains $_.ParentProcessId) -and ($seen -notcontains $_.ProcessId) })
    $tree += $children
    $frontier = @($children | ForEach-Object { $_.ProcessId })
    $seen += $frontier
  }
  return ,$tree
}

function Wait-Until([scriptblock]$condition, [int]$timeoutSec) {
  $deadline = (Get-Date).AddSeconds($timeoutSec)
  while ((Get-Date) -lt $deadline) {
    if (& $condition) { return $true }
    Start-Sleep -Milliseconds 200
  }
  return $false
}

function Last-PreviewEvent([string]$path) {
  if (-not (Test-Path $path)) { return "" }
  $line = Get-Content $path -Tail 200 -ErrorAction SilentlyContinue | Where-Object { $_ -match 'event=preview\.' } | Select-Object -Last 1
  if ($null -eq $line) { return "" }
  return ([regex]::Match($line, 'event=(\S+)')).Groups[1].Value
}

if (@(Get-Process -Name "tp-desktop" -ErrorAction SilentlyContinue).Count -gt 0) {
  throw "tp-desktop is already running; refusing to start a second instance."
}
foreach ($flag in "shot", "resize", "closepattern", "stop") { Remove-Item "$base.$flag" -ErrorAction SilentlyContinue }

$summary = [ordered]@{
  label = $Label; started = (Get-Date).ToString("s"); exe = $exePath
  exeSha256 = (Get-FileHash $exePath -Algorithm SHA256).Hash
  cpuCount = [Environment]::ProcessorCount; sampleSec = $SampleSec
}

$pattern = $null
if (-not $NoPattern) {
  $patternPath = (Resolve-Path (Join-Path $repo $PatternExe)).Path
  $pattern = Start-Process -FilePath $patternPath -PassThru -WindowStyle Normal `
    -ArgumentList @("--x", $PatternX, "--y", $PatternY, "--width", $PatternW, "--height", $PatternH, "--seconds", ($DurationSec + 120), "--topmost") `
    -RedirectStandardOutput "$base.pattern.stdout.log" -RedirectStandardError "$base.pattern.stderr.log"
  $summary.patternPid = $pattern.Id
  [void](Wait-Until { $pattern.Refresh(); $pattern.MainWindowHandle -ne [IntPtr]::Zero } 10)
}

$stderrLog = "$base.stderr.log"
$app = Start-Process -FilePath $exePath -WorkingDirectory $repo -PassThru `
  -RedirectStandardError $stderrLog -RedirectStandardOutput "$base.stdout.log"
$summary.pid = $app.Id
$summary.ipcObserved = Wait-Until {
  (Test-Path $stderrLog) -and ((Get-Content $stderrLog -Raw -ErrorAction SilentlyContinue) -match 'event=ipc\.get_app_info result=ok')
} 30
[void](Wait-Until { $app.Refresh(); $app.MainWindowHandle -ne [IntPtr]::Zero -and [TpQa]::IsWindowVisible($app.MainWindowHandle) } 10)
$hwnd = $app.MainWindowHandle

$csv = "$base.samples.csv"
"t_s,state_event,tree_cpu_pct,main_cpu_pct,main_ws_mb,tree_ws_mb,main_private_mb,main_handles,main_threads,tree_procs,gpu_max_engine_pct,gpu_3d_pct,tcp,udp" |
  Out-File -Encoding utf8 $csv
$started = Get-Date
$lastSample = $started.AddSeconds(-$SampleSec)
$prevCpu = @{}
$prevTime = $null
$shots = 0
$allPids = @($app.Id)
$summary.events = @()

try {
  while (((Get-Date) - $started).TotalSeconds -lt $DurationSec -and -not $app.HasExited) {
    if (Test-Path "$base.stop") { Remove-Item "$base.stop"; $summary.events += "stop requested at $([int]((Get-Date) - $started).TotalSeconds)s"; break }
    if (Test-Path "$base.shot") {
      Remove-Item "$base.shot"; $shots++
      foreach ($i in 1..4) { [void][TpQa]::DwmFlush() }
      $size = [TpQa]::Capture($hwnd, "$base.shot-$shots.png")
      $summary.events += "screenshot $shots ($size) at $([int]((Get-Date) - $started).TotalSeconds)s, state=$(Last-PreviewEvent $stderrLog)"
    }
    if ((Test-Path "$base.resize") -and $null -ne $pattern) {
      Remove-Item "$base.resize"
      $pattern.Refresh()
      # SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOMOVE
      [void][TpQa]::SetWindowPos($pattern.MainWindowHandle, [IntPtr]::Zero, 0, 0, 900, 560, 0x0004 -bor 0x0010 -bor 0x0002)
      $summary.events += "pattern resized to 900x560 at $([int]((Get-Date) - $started).TotalSeconds)s"
    }
    if ((Test-Path "$base.closepattern") -and $null -ne $pattern) {
      Remove-Item "$base.closepattern"
      [void]$pattern.CloseMainWindow()
      $summary.events += "pattern window closed at $([int]((Get-Date) - $started).TotalSeconds)s exited=$($pattern.WaitForExit(5000))"
    }

    $now = Get-Date
    if (($now - $lastSample).TotalSeconds -ge $SampleSec) {
      $lastSample = $now
      $tree = Get-Tree $app.Id
      $ids = @($tree | ForEach-Object { $_.ProcessId })
      $allPids = @($allPids + $ids | Select-Object -Unique)
      $procs = @($ids | ForEach-Object { Get-Process -Id $_ -ErrorAction SilentlyContinue })
      $cpuNow = @{}
      foreach ($pr in $procs) { try { $cpuNow[$pr.Id] = $pr.TotalProcessorTime.TotalMilliseconds } catch { } }
      $treeCpu = ""; $mainCpu = ""
      if ($null -ne $prevTime) {
        $wallMs = ($now - $prevTime).TotalMilliseconds * [Environment]::ProcessorCount
        $deltaAll = 0.0
        foreach ($k in $cpuNow.Keys) { if ($prevCpu.ContainsKey($k)) { $deltaAll += $cpuNow[$k] - $prevCpu[$k] } }
        $treeCpu = [math]::Round(100.0 * $deltaAll / $wallMs, 2)
        if ($cpuNow.ContainsKey($app.Id) -and $prevCpu.ContainsKey($app.Id)) {
          $mainCpu = [math]::Round(100.0 * ($cpuNow[$app.Id] - $prevCpu[$app.Id]) / $wallMs, 2)
        }
      }
      $prevCpu = $cpuNow; $prevTime = $now
      $main = $procs | Where-Object { $_.Id -eq $app.Id }
      $treeWs = [math]::Round((($procs | Measure-Object WorkingSet64 -Sum).Sum) / 1MB, 1)
      $gpuMax = ""; $gpu3d = ""
      if (-not $NoGpu) {
        try {
          $paths = @($ids | ForEach-Object { "\GPU Engine(pid_$($_)_*)\Utilization Percentage" })
          $samples = @((Get-Counter -Counter $paths -ErrorAction SilentlyContinue).CounterSamples)
          if ($samples.Count -gt 0) {
            $gpuMax = [math]::Round(($samples | Measure-Object CookedValue -Maximum).Maximum, 2)
            $gpu3d = [math]::Round((($samples | Where-Object { $_.InstanceName -match 'engtype_3d' } | Measure-Object CookedValue -Sum).Sum), 2)
          }
        } catch { $gpuMax = "err" }
      }
      $tcp = @(Get-NetTCPConnection -ErrorAction SilentlyContinue | Where-Object { $ids -contains $_.OwningProcess }).Count
      $udp = @(Get-NetUDPEndpoint -ErrorAction SilentlyContinue | Where-Object { $ids -contains $_.OwningProcess }).Count
      $row = @(
        [int]($now - $started).TotalSeconds, (Last-PreviewEvent $stderrLog), $treeCpu, $mainCpu,
        [math]::Round($main.WorkingSet64 / 1MB, 1), $treeWs, [math]::Round($main.PrivateMemorySize64 / 1MB, 1),
        $main.HandleCount, $main.Threads.Count, $procs.Count, $gpuMax, $gpu3d, $tcp, $udp
      ) -join ","
      $row | Out-File -Append -Encoding utf8 $csv
    }
    Start-Sleep -Milliseconds 500
  }
}
catch {
  $summary.scriptError = "$_"
}
finally {
  $summary.treePidsSeen = $allPids
  $summary.stateAtClose = Last-PreviewEvent $stderrLog
  $closeRequested = $false
  if (-not $app.HasExited) { $closeRequested = $app.CloseMainWindow() }
  $summary.closeMessageSent = $closeRequested
  $summary.exitedAfterClose = $app.WaitForExit($ExitTimeoutSec * 1000)
  if ($summary.exitedAfterClose) { $summary.exitCode = $app.ExitCode }
  if ($null -ne $pattern -and -not $pattern.HasExited) {
    [void]$pattern.CloseMainWindow()
    $summary.patternExited = $pattern.WaitForExit(5000)
  }
}

$gone = Wait-Until { @($allPids | Where-Object { Get-Process -Id $_ -ErrorAction SilentlyContinue }).Count -eq 0 } 10
$survivors = @($allPids | Where-Object { Get-Process -Id $_ -ErrorAction SilentlyContinue })
$summary.survivingPids = $survivors
if ($survivors.Count -gt 0) {
  $survivors | ForEach-Object { Stop-Process -Id $_ -Force -ErrorAction SilentlyContinue }
  $summary.forciblyStopped = $survivors
}
$summary.cleanExit = $gone -and $summary.exitedAfterClose -and $closeRequested
$summary.finished = (Get-Date).ToString("s")
$summary.previewEvents = @(Get-Content $stderrLog -ErrorAction SilentlyContinue | Where-Object { $_ -match 'event=(preview|app)\.' } | Select-Object -Last 400)
$json = $summary | ConvertTo-Json -Depth 5
[IO.File]::WriteAllText("$base.summary.json", $json, (New-Object System.Text.UTF8Encoding($false)))
"summary written: $base.summary.json cleanExit=$($summary.cleanExit)"
