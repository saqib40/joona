<#
.SYNOPSIS
    Builds the joona-engine sidecar and copies it to where Tauri expects it.

.DESCRIPTION
    Tauri's externalBin needs binaries/joona-engine-<target-triple>.exe next to tauri.conf.json.
    GPU variants are copied as joona-engine-<variant>-<triple>.exe; release builds bundle the
    CPU and Vulkan ones (see tauri.release.conf.json).

    The DLLs the engine needs that a clean Windows may lack (DirectML.dll, which ONNX Runtime
    imports, and the Visual C++ runtime) are copied to target\engine-runtime, which release
    builds install next to it.

.EXAMPLE
    .\scripts\build-engine.ps1
    .\scripts\build-engine.ps1 -Release
    .\scripts\build-engine.ps1 -Variant vulkan -Release
#>
param(
    [ValidateSet("cpu", "vulkan", "cuda")]
    [string]$Variant = "cpu",
    [switch]$Release,
    [string]$Target = "x86_64-pc-windows-msvc"
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot "dev-env.ps1")

$cargoArgs = @("build", "-p", "joona-engine", "--target", $Target)
if ($Release) { $cargoArgs += "--release" }
switch ($Variant) {
    "vulkan" { $cargoArgs += @("--features", "vulkan") }
    "cuda" { $cargoArgs += @("--features", "cuda") }
}

$targetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $root "target" }
if ($Variant -ne "cpu") {
    # Keep GPU builds apart so they don't invalidate the CPU build cache.
    $targetDir = Join-Path $targetDir "engine-$Variant"
    $cargoArgs += @("--target-dir", $targetDir)
}

Write-Host "cargo $($cargoArgs -join ' ')"
Push-Location $root
try {
    # cargo reports progress on stderr, which "Stop" would turn into a terminating error.
    $ErrorActionPreference = "Continue"
    & cargo @cargoArgs
    $exitCode = $LASTEXITCODE
    $ErrorActionPreference = "Stop"
    if ($exitCode -ne 0) { throw "cargo build failed with exit code $exitCode" }
}
finally {
    Pop-Location
}

$profileDir = if ($Release) { "release" } else { "debug" }
$built = Join-Path $targetDir "$Target\$profileDir\joona-engine.exe"
$binDir = Join-Path $root "apps\desktop\src-tauri\binaries"
New-Item -ItemType Directory -Force $binDir | Out-Null

$name = if ($Variant -eq "cpu") { "joona-engine-$Target.exe" } else { "joona-engine-$Variant-$Target.exe" }
$dest = Join-Path $binDir $name
Copy-Item $built $dest -Force
Write-Host "Engine copied to $dest"

# The one under the newest MSVC version folder.
function Select-Newest {
    $input | Where-Object FullName -Match '\\MSVC\\[\d.]+\\' |
        Sort-Object { [version]($_.FullName -replace '^.*\\MSVC\\([\d.]+)\\.*$', '$1') } |
        Select-Object -Last 1
}
$vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
$vs = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
$dumpbin = Get-ChildItem (Join-Path $vs "VC\Tools\MSVC\*\bin\Hostx64\x64\dumpbin.exe") | Select-Newest
$crt = Get-ChildItem (Join-Path $vs "VC\Redist\MSVC\*\x64\Microsoft.VC*.CRT") -Directory | Select-Newest
if (-not $dumpbin -or -not $crt) { throw "dumpbin or the Visual C++ runtime wasn't found in $vs." }

$runtimeDir = Join-Path $root "target\engine-runtime"
New-Item -ItemType Directory -Force $runtimeDir | Out-Null
$profileOut = Split-Path $built
$imports = & $dumpbin.FullName /nologo /dependents $built | ForEach-Object { $_.Trim() } | Where-Object { $_ -match '^[\w.-]+\.dll$' }
foreach ($dll in $imports) {
    if ($dll -eq "DirectML.dll") {
        # ort-sys copies it into the profile's build folder.
        $candidates = @((Join-Path $profileOut "build\$dll"), (Join-Path $profileOut $dll))
    }
    elseif ($dll -match '^(msvcp|vcruntime|concrt|vccorlib)140') {
        $candidates = @(Join-Path $crt.FullName $dll)
    }
    else {
        continue
    }
    $source = $candidates | Where-Object { Test-Path $_ } | Select-Object -First 1
    if (-not $source) { throw "The engine needs $dll, but it isn't at $($candidates -join ' or ')." }
    Copy-Item $source $runtimeDir -Force
    Write-Host "Runtime $dll copied to $runtimeDir"
}
