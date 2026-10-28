<#
.SYNOPSIS
  BOOT-01 Windows runtime smoke test for the TP Remote desktop app (WIN-BOOT-01..08).

.DESCRIPTION
  Starts one tp-desktop.exe, waits (bounded, condition-based) for the UI to call
  get_app_info, records the process tree, listening sockets, token elevation and DPI
  awareness, captures the app window (PrintWindow: the app's own window only, never the
  screen), optionally resizes/moves it, closes it with WM_CLOSE and verifies that no
  process of its tree survives. Only processes started by this script are ever stopped.

.EXAMPLE
  powershell -NoProfile -File scripts/smoke-windows.ps1 -Exe target\release\tp-desktop.exe -Label release-run1 -Resize
  powershell -NoProfile -File scripts/smoke-windows.ps1 -Exe target\debug\tp-desktop.exe -Label debug-fault -InjectFault
#>
param(
  [string]$Exe = "target\release\tp-desktop.exe",
  [string]$Label = "run",
  [string]$EvidenceDir = "docs\qa\evidence\BOOT-01\windows",
  [switch]$InjectFault,
  [switch]$Resize,
  [int]$StartTimeoutSec = 30,
  [int]$ExitTimeoutSec = 15
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version 3

$repo = Split-Path -Parent $PSScriptRoot
$exePath = (Resolve-Path (Join-Path $repo $Exe)).Path
$outDir = Join-Path $repo $EvidenceDir
New-Item -ItemType Directory -Force $outDir | Out-Null

Add-Type -ReferencedAssemblies System.Drawing -TypeDefinition @"
using System;
using System.Drawing;
using System.Drawing.Imaging;
using System.Runtime.InteropServices;

public static class TpSmoke {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint flags);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int cx, int cy, uint flags);
  [DllImport("user32.dll")] public static extern IntPtr GetWindowDpiAwarenessContext(IntPtr h);
  [DllImport("user32.dll")] public static extern int GetAwarenessFromDpiAwarenessContext(IntPtr c);
  [DllImport("user32.dll")] public static extern bool AreDpiAwarenessContextsEqual(IntPtr a, IntPtr b);
  [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("dwmapi.dll")] public static extern int DwmFlush();
  [DllImport("advapi32.dll", SetLastError = true)] public static extern bool OpenProcessToken(IntPtr p, uint access, out IntPtr tok);
  [DllImport("advapi32.dll", SetLastError = true)] public static extern bool GetTokenInformation(IntPtr tok, int cls, IntPtr info, int len, out int retLen);
  [DllImport("advapi32.dll")] public static extern IntPtr GetSidSubAuthority(IntPtr sid, uint idx);
  [DllImport("advapi32.dll")] public static extern IntPtr GetSidSubAuthorityCount(IntPtr sid);
  [DllImport("kernel32.dll")] public static extern bool CloseHandle(IntPtr h);

  public static int[] Rect(IntPtr h) {
    RECT r; GetWindowRect(h, out r);
    return new int[] { r.Left, r.Top, r.Right - r.Left, r.Bottom - r.Top };
  }

  // PW_RENDERFULLCONTENT (2) is required for DirectComposition content such as WebView2.
  public static string Capture(IntPtr h, string path) {
    int[] r = Rect(h);
    using (Bitmap bmp = new Bitmap(r[2], r[3], PixelFormat.Format32bppArgb)) {
      using (Graphics g = Graphics.FromImage(bmp)) {
        IntPtr hdc = g.GetHdc();
        bool ok = PrintWindow(h, hdc, 2);
        g.ReleaseHdc(hdc);
        if (!ok) { return "PrintWindow failed"; }
      }
      bmp.Save(path, ImageFormat.Png);
    }
    return r[2] + "x" + r[3];
  }

  public static string DpiAwareness(IntPtr h) {
    IntPtr ctx = GetWindowDpiAwarenessContext(h);
    bool pmv2 = AreDpiAwarenessContextsEqual(ctx, new IntPtr(-4));
    int awareness = GetAwarenessFromDpiAwarenessContext(ctx);
    string name = awareness == 0 ? "unaware" : awareness == 1 ? "system" : awareness == 2 ? "per-monitor" : "unknown";
    return name + (pmv2 ? " (PerMonitorV2)" : "") + ", window DPI " + GetDpiForWindow(h);
  }

  public static string Token(IntPtr process) {
    IntPtr tok;
    if (!OpenProcessToken(process, 0x0008, out tok)) { return "OpenProcessToken failed: " + Marshal.GetLastWin32Error(); }
    try {
      int len;
      IntPtr buf = Marshal.AllocHGlobal(4);
      GetTokenInformation(tok, 20, buf, 4, out len);
      bool elevated = Marshal.ReadInt32(buf) != 0;
      Marshal.FreeHGlobal(buf);
      GetTokenInformation(tok, 25, IntPtr.Zero, 0, out len);
      buf = Marshal.AllocHGlobal(len);
      try {
        if (!GetTokenInformation(tok, 25, buf, len, out len)) { return "elevated=" + elevated + " integrity=unknown"; }
        IntPtr sid = Marshal.ReadIntPtr(buf);
        int count = Marshal.ReadByte(GetSidSubAuthorityCount(sid));
        int rid = Marshal.ReadInt32(GetSidSubAuthority(sid, (uint)(count - 1)));
        string level = rid >= 0x4000 ? "System" : rid >= 0x3000 ? "High" : rid >= 0x2000 ? "Medium" : rid >= 0x1000 ? "Low" : "Untrusted";
        return "elevated=" + elevated + " integrity=" + level + " (0x" + rid.ToString("X") + ")";
      } finally { Marshal.FreeHGlobal(buf); }
    } finally { CloseHandle(tok); }
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

function Wait-Until([scriptblock]$condition, [int]$timeoutSec, [string]$what) {
  $deadline = (Get-Date).AddSeconds($timeoutSec)
  while ((Get-Date) -lt $deadline) {
    if (& $condition) { return $true }
    Start-Sleep -Milliseconds 200
  }
  Write-Warning "timed out after ${timeoutSec}s waiting for: $what"
  return $false
}

$base = Join-Path $outDir "$Label"
$stderrLog = "$base.stderr.log"
$stdoutLog = "$base.stdout.log"
$result = [ordered]@{
  label = $Label; started = (Get-Date).ToString("s"); exe = $exePath
  exeSha256 = (Get-FileHash $exePath -Algorithm SHA256).Hash; injectFault = [bool]$InjectFault
}

$existing = @(Get-Process -Name "tp-desktop" -ErrorAction SilentlyContinue)
if ($existing.Count -gt 0) {
  throw "tp-desktop is already running (PIDs $($existing.Id -join ', ')); refusing to start a second instance."
}

if ($InjectFault) { $env:TP_REMOTE_DEV_FAIL_APP_INFO = "1" }
try {
  $p = Start-Process -FilePath $exePath -WorkingDirectory $repo -PassThru `
    -RedirectStandardError $stderrLog -RedirectStandardOutput $stdoutLog
} finally {
  Remove-Item Env:TP_REMOTE_DEV_FAIL_APP_INFO -ErrorAction SilentlyContinue
}
$result.pid = $p.Id
$launchedAt = Get-Date
$allPids = @($p.Id)

try {

$expectedIpc = if ($InjectFault) { "event=ipc.get_app_info result=err" } else { "event=ipc.get_app_info result=ok" }
$ipcSeen = Wait-Until {
  if ($p.HasExited) { return $true }
  (Test-Path $stderrLog) -and ((Get-Content $stderrLog -Raw -ErrorAction SilentlyContinue) -match [regex]::Escape($expectedIpc))
} $StartTimeoutSec "'$expectedIpc' in stderr"
$result.exitedDuringStartup = $p.HasExited
$result.ipcObserved = $ipcSeen -and -not $p.HasExited
$result.secondsToIpc = [math]::Round(((Get-Date) - $launchedAt).TotalSeconds, 2)

$windowReady = Wait-Until { $p.Refresh(); $p.MainWindowHandle -ne [IntPtr]::Zero -and [TpSmoke]::IsWindowVisible($p.MainWindowHandle) } 10 "visible main window"
$hwnd = $p.MainWindowHandle
$result.windowVisible = $windowReady
$result.windowTitle = $p.MainWindowTitle

if ($windowReady) {
  $result.token = [TpSmoke]::Token($p.Handle)
  $result.dpiAwareness = [TpSmoke]::DpiAwareness($hwnd)
  $result.initialRect = [TpSmoke]::Rect($hwnd) -join ","
  [void][TpSmoke]::DwmFlush(); [void][TpSmoke]::DwmFlush()
  $result.screenshotInitial = [TpSmoke]::Capture($hwnd, "$base.initial.png")

  $tree = Get-Tree $p.Id
  $result.processTree = @($tree | ForEach-Object { "$($_.Name)#$($_.ProcessId)<-$($_.ParentProcessId)" })
  $treeIds = @($tree | ForEach-Object { $_.ProcessId })
  $result.tcpListeners = @(Get-NetTCPConnection -State Listen -ErrorAction SilentlyContinue |
      Where-Object { $treeIds -contains $_.OwningProcess } |
      ForEach-Object { "$($_.LocalAddress):$($_.LocalPort) pid=$($_.OwningProcess)" })
  $result.udpEndpoints = @(Get-NetUDPEndpoint -ErrorAction SilentlyContinue |
      Where-Object { $treeIds -contains $_.OwningProcess } |
      ForEach-Object { "$($_.LocalAddress):$($_.LocalPort) pid=$($_.OwningProcess)" })

  if ($Resize) {
    # SWP_NOZORDER | SWP_NOACTIVATE
    $flags = 0x0004 -bor 0x0010
    $cases = @(
      @{ name = "below-min-500x400"; x = 100; y = 100; w = 500; h = 400 },
      @{ name = "1024x700"; x = 100; y = 100; w = 1024; h = 700 },
      @{ name = "1600x1000"; x = 100; y = 40; w = 1600; h = 1000 },
      @{ name = "left-monitor-1100x760"; x = -1820; y = 171; w = 1100; h = 760 }
    )
    $result.resize = @()
    foreach ($c in $cases) {
      [void][TpSmoke]::SetWindowPos($hwnd, [IntPtr]::Zero, $c.x, $c.y, $c.w, $c.h, $flags)
      [void](Wait-Until { $r = [TpSmoke]::Rect($hwnd); ($r[0] -eq $c.x) -and ($r[1] -eq $c.y) } 5 "window moved for $($c.name)")
      # Let DWM and WebView2 present a frame at the new size before capturing it.
      foreach ($i in 1..6) { [void][TpSmoke]::DwmFlush() }
      Start-Sleep -Milliseconds 400
      $actual = [TpSmoke]::Rect($hwnd) -join ","
      $shot = [TpSmoke]::Capture($hwnd, "$base.resize-$($c.name).png")
      $result.resize += "$($c.name): requested=$($c.x),$($c.y),$($c.w),$($c.h) actual=$actual dpi=$([TpSmoke]::GetDpiForWindow($hwnd)) shot=$shot"
    }
  }
}

$allPids = @(Get-Tree $p.Id | ForEach-Object { $_.ProcessId })
}
catch {
  $result.scriptError = "$_"
}
finally {
  # Always close what this run started, even if a check above failed.
  $result.treePidsBeforeClose = $allPids
  $closeRequested = $false
  if (-not $p.HasExited) { $closeRequested = $p.CloseMainWindow() }
  $result.closeMessageSent = $closeRequested
  $result.exitedAfterClose = $p.WaitForExit($ExitTimeoutSec * 1000)
  if ($result.exitedAfterClose) { $result.exitCode = $p.ExitCode }
}

$orphansGone = Wait-Until {
  @($allPids | Where-Object { Get-Process -Id $_ -ErrorAction SilentlyContinue }).Count -eq 0
} 10 "all processes of the tree to exit"
$survivors = @($allPids | Where-Object { Get-Process -Id $_ -ErrorAction SilentlyContinue })
$result.survivingPids = $survivors
if ($survivors.Count -gt 0) {
  # Only processes from this run's tree are stopped.
  $survivors | ForEach-Object { Stop-Process -Id $_ -Force -ErrorAction SilentlyContinue }
  $result.forciblyStopped = $survivors
}
$result.cleanExit = $orphansGone -and $result.exitedAfterClose -and $closeRequested

$stderr = if (Test-Path $stderrLog) { @(Get-Content $stderrLog | ForEach-Object { [string]$_ }) } else { @() }
$result.stderrEvents = @($stderr | Where-Object { $_ -match '^tp-remote ' })
$result.stderrOther = @($stderr | Where-Object { $_ -notmatch '^tp-remote ' } | Select-Object -First 20)
$result.finished = (Get-Date).ToString("s")

$json = $result | ConvertTo-Json -Depth 5
[IO.File]::WriteAllText("$base.summary.json", $json, (New-Object System.Text.UTF8Encoding($false)))
$json
