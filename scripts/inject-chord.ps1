# Simulates dictation gestures with keybd_event, for testing the hotkey hook.
# .\scripts\inject-chord.ps1 -Gesture hold -HoldMs 800
param(
    [ValidateSet("hold", "space", "tap", "doubletap")]
    [string]$Gesture = "hold",
    [int]$HoldMs = 800,
    [int]$DelayMs = 1500
)

Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class JoonaKeys {
    [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra);
}
"@

$KEYUP = 0x2
$EXT = 0x1
function Down($vk, $flags = 0) { [JoonaKeys]::keybd_event($vk, 0, $flags, [UIntPtr]::Zero) }
function Up($vk, $flags = 0) { [JoonaKeys]::keybd_event($vk, 0, $flags -bor $KEYUP, [UIntPtr]::Zero) }

$LCTRL = 0xA2; $LWIN = 0x5B; $SPACE = 0x20

Start-Sleep -Milliseconds $DelayMs
switch ($Gesture) {
    "hold" {
        Down $LCTRL; Start-Sleep -Milliseconds 30; Down $LWIN $EXT
        Start-Sleep -Milliseconds $HoldMs
        Up $LWIN $EXT; Start-Sleep -Milliseconds 20; Up $LCTRL
    }
    "space" {
        Down $LCTRL; Start-Sleep -Milliseconds 30; Down $LWIN $EXT
        Start-Sleep -Milliseconds 200
        Down $SPACE; Start-Sleep -Milliseconds 40; Up $SPACE
        Start-Sleep -Milliseconds 150
        Up $LWIN $EXT; Up $LCTRL
        Start-Sleep -Milliseconds $HoldMs
        Down $LCTRL; Down $LWIN $EXT; Start-Sleep -Milliseconds 60; Up $LWIN $EXT; Up $LCTRL
    }
    "tap" {
        Down $LCTRL; Down $LWIN $EXT; Start-Sleep -Milliseconds 120; Up $LWIN $EXT; Up $LCTRL
    }
    "doubletap" {
        Down $LCTRL; Down $LWIN $EXT; Start-Sleep -Milliseconds 100; Up $LWIN $EXT; Up $LCTRL
        Start-Sleep -Milliseconds 150
        Down $LCTRL; Down $LWIN $EXT; Start-Sleep -Milliseconds 100; Up $LWIN $EXT; Up $LCTRL
        Start-Sleep -Milliseconds $HoldMs
        Down $LCTRL; Down $LWIN $EXT; Start-Sleep -Milliseconds 80; Up $LWIN $EXT; Up $LCTRL
    }
}
