<#
  CAP-02A-R2 diagnostic (read-only): logs window minimize/restore and foreground changes
  for -Seconds, with the window class, title and owning process, using an out-of-context
  WinEvent hook. It observes only; it sends no input and changes no window.
    powershell -NoProfile -File docs\qa\evidence\CAP-02A-R2\app\watch-window-events.ps1 -Seconds 180 -Log <file>
#>
param([int]$Seconds = 180, [string]$Log)
$ErrorActionPreference = "Stop"
Add-Type -TypeDefinition @"
using System; using System.Diagnostics; using System.IO; using System.Runtime.InteropServices; using System.Text;
public static class TpWatch {
  delegate void WinEventProc(IntPtr hook, uint ev, IntPtr hwnd, int idObject, int idChild, uint thread, uint time);
  [DllImport("user32.dll")] static extern IntPtr SetWinEventHook(uint min, uint max, IntPtr mod, WinEventProc proc, uint pid, uint tid, uint flags);
  [DllImport("user32.dll")] static extern bool UnhookWinEvent(IntPtr h);
  [DllImport("user32.dll")] static extern int GetMessageW(out MSG m, IntPtr h, uint a, uint b);
  [DllImport("user32.dll")] static extern bool PeekMessageW(out MSG m, IntPtr h, uint a, uint b, uint remove);
  [DllImport("user32.dll")] static extern bool TranslateMessage(ref MSG m);
  [DllImport("user32.dll")] static extern IntPtr DispatchMessageW(ref MSG m);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetWindowTextW(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassNameW(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [StructLayout(LayoutKind.Sequential)] struct MSG { public IntPtr h; public uint m; public IntPtr w; public IntPtr l; public uint t; public int x; public int y; }
  static StreamWriter log; static WinEventProc keep;
  static string Describe(IntPtr h) { var t = new StringBuilder(120); var c = new StringBuilder(80); GetWindowTextW(h, t, 120); GetClassNameW(h, c, 80); uint pid; GetWindowThreadProcessId(h, out pid); string name = "?"; try { name = Process.GetProcessById((int)pid).ProcessName; } catch {} return "hwnd=" + h + " class=" + c + " title='" + t + "' process=" + name + "#" + pid; }
  static void On(IntPtr hook, uint ev, IntPtr hwnd, int idObject, int idChild, uint thread, uint time) {
    if (idObject != 0) return;
    string kind = ev == 0x0003 ? "FOREGROUND" : ev == 0x0016 ? "MINIMIZE_START" : ev == 0x0017 ? "MINIMIZE_END" : ev.ToString("X");
    log.WriteLine(DateTime.Now.ToString("HH:mm:ss.fff") + " " + kind + " " + Describe(hwnd)); log.Flush();
  }
  public static void Run(int seconds, string path) {
    log = new StreamWriter(path, true, new UTF8Encoding(false)); keep = On;
    IntPtr h1 = SetWinEventHook(0x0003, 0x0003, IntPtr.Zero, keep, 0, 0, 0);
    IntPtr h2 = SetWinEventHook(0x0016, 0x0017, IntPtr.Zero, keep, 0, 0, 0);
    var end = DateTime.Now.AddSeconds(seconds); MSG m;
    while (DateTime.Now < end) { while (PeekMessageW(out m, IntPtr.Zero, 0, 0, 1)) { TranslateMessage(ref m); DispatchMessageW(ref m); } System.Threading.Thread.Sleep(20); }
    UnhookWinEvent(h1); UnhookWinEvent(h2); log.WriteLine(DateTime.Now.ToString("HH:mm:ss.fff") + " watch ended"); log.Close();
  }
}
"@
"$(Get-Date -Format 'HH:mm:ss.fff') watch started for $Seconds s" | Out-File -Encoding utf8 $Log
[TpWatch]::Run($Seconds, (Resolve-Path $Log).Path)
