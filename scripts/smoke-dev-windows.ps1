<#
.SYNOPSIS
  BOOT-01 check of the local development launch (`npm run dev` = `tauri dev`) on Windows.

.DESCRIPTION
  Starts `npm run dev` as a new process tree, waits (bounded, condition-based) for the
  dev build of tp-desktop to answer get_app_info, records the Vite listener address,
  captures the app window, closes the app with WM_CLOSE and verifies the whole tree
  (npm, Vite, cargo, app, WebView2) exits. Only processes of this tree are ever stopped.
#>
param(
  [string]$Label = "dev-launch",
  [string]$EvidenceDir = "docs\qa\evidence\BOOT-01\windows",
  [int]$StartTimeoutSec = 420,
  [int]$ExitTimeoutSec = 30
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version 3

$repo = Split-Path -Parent $PSScriptRoot
$outDir = Join-Path $repo $EvidenceDir
New-Item -ItemType Directory -Force $outDir | Out-Null
$base = Join-Path $outDir $Label
$log = "$base.log"
$errLog = "$base.err.log"

Add-Type -ReferencedAssemblies System.Drawing -TypeDefinition @"
using System;
using System.Drawing;
using System.Drawing.Imaging;
using System.Runtime.InteropServices;
public static class TpDevSmoke {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint flags);
  [DllImport("dwmapi.dll")] public static extern int DwmFlush();
  public static string Capture(IntPtr h, string path) {
    RECT r; GetWindowRect(h, out r);
    using (Bitmap bmp = new Bitmap(r.Right - r.Left, r.Bottom - r.Top, PixelFormat.Format32bppArgb)) {
      using (Graphics g = Graphics.FromImage(bmp)) { IntPtr hdc = g.GetHdc(); PrintWindow(h, hdc, 2); g.ReleaseHdc(hdc); }
      bmp.Save(path, ImageFormat.Png);
    }
    return (r.Right - r.Left) + "x" + (r.Bottom - r.Top);
  }
}
"@

function Get-Tree([int]$rootPid) {
  $all = @(Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId, Name, ExecutablePath)
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

$result = [ordered]@{ label = $Label; started = (Get-Date).ToString("s") }

$busy = @(Get-NetTCPConnection -LocalPort 1420 -State Listen -ErrorAction SilentlyContinue)
if ($busy.Count -gt 0) { throw "port 1420 is already in use by PID $($busy[0].OwningProcess); not starting." }
if (@(Get-Process -Name tp-desktop -ErrorAction SilentlyContinue).Count -gt 0) { throw "tp-desktop is already running; not starting." }

$root = Start-Process -FilePath "cmd.exe" -ArgumentList "/d", "/c", "npm run dev" -WorkingDirectory $repo `
  -PassThru -RedirectStandardOutput $log -RedirectStandardError $errLog -WindowStyle Hidden
$result.rootPid = $root.Id
$launched = Get-Date
$app = $null
$seenPids = @($root.Id)

try {
  $deadline = $launched.AddSeconds($StartTimeoutSec)
  $ready = $false
  while ((Get-Date) -lt $deadline -and -not $root.HasExited) {
    $tree = Get-Tree $root.Id
    $seenPids = @($seenPids + @($tree | ForEach-Object { $_.ProcessId }) | Select-Object -Unique)
    $node = @($tree | Where-Object { $_.Name -eq "tp-desktop.exe" }) | Select-Object -First 1
    $text = (Get-Content $log, $errLog -Raw -ErrorAction SilentlyContinue) -join "`n"
    if ($node -and $text -match "event=ipc\.get_app_info result=ok") {
      $app = Get-Process -Id $node.ProcessId
      $app.Refresh()
      if ($app.MainWindowHandle -ne [IntPtr]::Zero) { $ready = $true; break }
    }
    Start-Sleep -Milliseconds 500
  }
  $result.ready = $ready
  $result.secondsToReady = [math]::Round(((Get-Date) - $launched).TotalSeconds, 1)

  if ($ready) {
    $result.appPid = $app.Id
    $result.appExe = $app.Path
    $result.viteListeners = @(Get-NetTCPConnection -LocalPort 1420 -State Listen -ErrorAction SilentlyContinue |
        ForEach-Object { "$($_.LocalAddress):$($_.LocalPort) pid=$($_.OwningProcess)" })
    $tree = Get-Tree $root.Id
    $treeIds = @($tree | ForEach-Object { $_.ProcessId })
    $result.processTree = @($tree | ForEach-Object { "$($_.Name)#$($_.ProcessId)<-$($_.ParentProcessId)" })
    $result.otherTreeListeners = @(Get-NetTCPConnection -State Listen -ErrorAction SilentlyContinue |
        Where-Object { ($treeIds -contains $_.OwningProcess) -and $_.LocalPort -ne 1420 } |
        ForEach-Object { "$($_.LocalAddress):$($_.LocalPort) pid=$($_.OwningProcess)" })
    foreach ($i in 1..4) { [void][TpDevSmoke]::DwmFlush() }
    $result.screenshot = [TpDevSmoke]::Capture($app.MainWindowHandle, "$base.png")
  }
}
catch {
  $result.scriptError = "$_"
}
finally {
  $seenPids = @($seenPids + @(Get-Tree $root.Id | ForEach-Object { $_.ProcessId }) | Select-Object -Unique)
  $result.treePids = $seenPids
  if ($app -and -not $app.HasExited) {
    $result.closeMessageSent = $app.CloseMainWindow()
    $result.appExited = $app.WaitForExit(15000)
  }
  $result.rootExited = $root.WaitForExit($ExitTimeoutSec * 1000)
  $survivorDeadline = (Get-Date).AddSeconds(10)
  do {
    $survivors = @($seenPids | Where-Object { Get-Process -Id $_ -ErrorAction SilentlyContinue })
    if ($survivors.Count -eq 0) { break }
    Start-Sleep -Milliseconds 300
  } while ((Get-Date) -lt $survivorDeadline)
  $result.survivorsAfterClose = @($survivors | ForEach-Object {
      $sp = Get-Process -Id $_ -ErrorAction SilentlyContinue; if ($sp) { "$($sp.ProcessName)#$_" } })
  if ($survivors.Count -gt 0) {
    # Stop only what this run started.
    $survivors | ForEach-Object { Stop-Process -Id $_ -Force -ErrorAction SilentlyContinue }
    $result.forciblyStopped = $survivors
  }
  $result.port1420FreeAfter = @(Get-NetTCPConnection -LocalPort 1420 -State Listen -ErrorAction SilentlyContinue).Count -eq 0
  $result.finished = (Get-Date).ToString("s")
}

$text = (Get-Content $log, $errLog -ErrorAction SilentlyContinue | ForEach-Object { [string]$_ })
$result.appEvents = @($text | Where-Object { $_ -match '^tp-remote ' })
$json = $result | ConvertTo-Json -Depth 5
[IO.File]::WriteAllText("$base.summary.json", $json, (New-Object System.Text.UTF8Encoding($false)))
$json
