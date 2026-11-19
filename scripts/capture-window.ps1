# Saves a PNG of a top-level window, for checking UI changes during development.
# .\scripts\capture-window.ps1 -Title "Joona" -Out "$env:TEMP\joona.png"
param(
    [string]$Title = "Joona",
    [string]$Out = (Join-Path $env:TEMP "joona-window.png"),
    [string]$Process = "joona"
)

Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class JoonaWin32 {
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint flags);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr p);
    [DllImport("user32.dll")] public static extern int GetWindowText(IntPtr h, System.Text.StringBuilder s, int n);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
    public delegate bool EnumProc(IntPtr h, IntPtr p);
}
"@

[JoonaWin32]::SetProcessDPIAware() | Out-Null
$pids = @(Get-Process $Process -ErrorAction SilentlyContinue | ForEach-Object { $_.Id })
$found = [IntPtr]::Zero
$cb = [JoonaWin32+EnumProc] {
    param($h, $p)
    $sb = New-Object System.Text.StringBuilder 256
    [JoonaWin32]::GetWindowText($h, $sb, 256) | Out-Null
    $procId = 0
    [JoonaWin32]::GetWindowThreadProcessId($h, [ref]$procId) | Out-Null
    if ($sb.ToString() -eq $Title -and $pids -contains [int]$procId -and [JoonaWin32]::IsWindowVisible($h)) {
        $script:found = $h
        return $false
    }
    return $true
}
[JoonaWin32]::EnumWindows($cb, [IntPtr]::Zero) | Out-Null
if ($found -eq [IntPtr]::Zero) { throw "No visible window titled '$Title' found for process '$Process'" }

$r = New-Object JoonaWin32+RECT
[JoonaWin32]::GetWindowRect($found, [ref]$r) | Out-Null
$w = $r.Right - $r.Left
$h = $r.Bottom - $r.Top
$bmp = New-Object System.Drawing.Bitmap $w, $h
$g = [System.Drawing.Graphics]::FromImage($bmp)
$hdc = $g.GetHdc()
# PW_RENDERFULLCONTENT (2) captures WebView2 content.
[JoonaWin32]::PrintWindow($found, $hdc, 2) | Out-Null
$g.ReleaseHdc($hdc)
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose()
$bmp.Dispose()
Write-Output $Out
