# Drives joona-engine directly over its stdin/stdout protocol with a WAV file
# (16 kHz mono PCM16), printing every line the engine writes.
# .\scripts\engine-smoke.ps1 -Wav test.wav -Model C:\...\ggml-tiny.bin
param(
    [Parameter(Mandatory)] [string]$Wav,
    [Parameter(Mandatory)] [string]$Model,
    [string]$Engine = "whisper",
    [string]$ModelId = "test-model",
    [string]$Vad = "$env:LOCALAPPDATA\Joona\models\silero-vad\silero_vad_v4.onnx",
    [string]$Task = "transcribe",
    [string]$Language = "",
    [string[]]$Candidates = @(),
    [string]$Exe = "$PSScriptRoot\..\apps\desktop\src-tauri\binaries\joona-engine-x86_64-pc-windows-msvc.exe"
)

$psi = New-Object System.Diagnostics.ProcessStartInfo
$psi.FileName = (Resolve-Path $Exe).Path
$psi.UseShellExecute = $false
$psi.RedirectStandardInput = $true
$psi.RedirectStandardOutput = $true
$psi.RedirectStandardError = $true
$psi.CreateNoWindow = $true
$p = [System.Diagnostics.Process]::Start($psi)
$stderr = $p.StandardError.ReadToEndAsync()

function Send($obj) {
    $line = $obj | ConvertTo-Json -Compress -Depth 6
    $p.StandardInput.WriteLine($line)
    $p.StandardInput.Flush()
}
function Receive([int]$id) {
    while ($true) {
        $line = $p.StandardOutput.ReadLine()
        if ($null -eq $line) { throw "engine exited" }
        if ($line.Length -gt 400) { Write-Host ("<- " + $line.Substring(0, 400) + "...") } else { Write-Host "<- $line" }
        $msg = $line | ConvertFrom-Json
        if ($id -eq 0 -or $msg.id -eq $id) { return $msg }
    }
}

$sw = [System.Diagnostics.Stopwatch]::StartNew()
Receive 0 | Out-Null
Send @{ id = 1; method = "configure"; params = @{ vad_model = $Vad } }
Receive 1 | Out-Null
Send @{ id = 2; method = "load"; params = @{ model = @{ id = $ModelId; engine = $Engine; path = $Model }; use_gpu = $false } }
Receive 2 | Out-Null

$open = @{ model = $ModelId; task = $Task }
if ($Language) { $open.language = $Language }
if ($Candidates.Count -gt 0) { $open.candidates = $Candidates }
Send @{ id = 3; method = "open"; params = $open }
$session = (Receive 3).result.session

$bytes = [System.IO.File]::ReadAllBytes((Resolve-Path $Wav).Path)
# Find the "data" chunk instead of assuming a 44-byte header.
$pos = 12
while ($pos -lt $bytes.Length - 8) {
    $tag = [System.Text.Encoding]::ASCII.GetString($bytes, $pos, 4)
    $len = [BitConverter]::ToInt32($bytes, $pos + 4)
    if ($tag -eq "data") { $pos += 8; break }
    $pos += 8 + $len
}
$chunk = 6400 # 200 ms of 16 kHz PCM16
for ($i = $pos; $i -lt $bytes.Length; $i += $chunk) {
    $n = [Math]::Min($chunk, $bytes.Length - $i)
    Send @{ method = "feed"; params = @{ session = $session; pcm16 = [Convert]::ToBase64String($bytes, $i, $n) } }
}
$t0 = $sw.ElapsedMilliseconds
Send @{ id = 4; method = "finish"; params = @{ session = $session } }
$result = Receive 4
Write-Host ("finish round trip: {0} ms" -f ($sw.ElapsedMilliseconds - $t0))
Send @{ id = 5; method = "status" }
Receive 5 | Out-Null
Send @{ id = 6; method = "shutdown" }
Receive 6 | Out-Null
$p.WaitForExit(5000) | Out-Null
Write-Host "exit code: $($p.ExitCode)"
$err = $stderr.Result
if ($err) { Write-Host "--- stderr (last 15 lines) ---"; ($err -split "`n" | Select-Object -Last 15) -join "`n" | Write-Host }
