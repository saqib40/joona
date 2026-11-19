# Puts cargo and CMake on PATH and points bindgen at libclang for the current
# PowerShell session. Dot-source it: . .\scripts\dev-env.ps1

$cargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
if ((Test-Path $cargoBin) -and -not (($env:Path -split ';') -contains $cargoBin)) {
    $env:Path = "$cargoBin;$env:Path"
}

if (-not (Get-Command cmake -ErrorAction SilentlyContinue)) {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
    if (Test-Path $vswhere) {
        $vs = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
        if ($vs) {
            $cmakeBin = Join-Path $vs "Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin"
            if (Test-Path $cmakeBin) {
                $env:Path = "$cmakeBin;$env:Path"
            }
        }
    }
}

if (-not (Get-Command cmake -ErrorAction SilentlyContinue)) {
    Write-Warning "CMake was not found. Run scripts\setup-dev.ps1 or install it with: winget install Kitware.CMake"
}

# whisper-rs-sys generates its bindings with bindgen, which loads libclang.dll.
if (-not $env:LIBCLANG_PATH) {
    $candidates = @(
        (Join-Path $env:ProgramFiles "LLVM\bin"),
        (Join-Path $env:LOCALAPPDATA "joona-dev\libclang")
    )
    foreach ($dir in $candidates) {
        if (Test-Path (Join-Path $dir "libclang.dll")) {
            $env:LIBCLANG_PATH = $dir
            break
        }
    }
}
if (-not $env:LIBCLANG_PATH) {
    Write-Warning "libclang was not found. Run scripts\setup-dev.ps1 or install LLVM with: winget install LLVM.LLVM"
}

# MSBuild fails on paths longer than 260 characters, so keep the cargo target directory short.
if ($env:CARGO_TARGET_DIR -and $env:CARGO_TARGET_DIR.Length -gt 60) {
    $env:CARGO_TARGET_DIR = Join-Path (Split-Path -Parent $PSScriptRoot) "target"
}

# ort-sys 2.0.0-rc.12 copies DirectML.dll into <profile>\build, <profile>\build\examples and
# <profile>\build\deps under Cargo's current build-directory layout, and its build script
# fails when those folders don't exist yet.
$targetRoot = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path (Split-Path -Parent $PSScriptRoot) "target" }
foreach ($profile in "debug", "release", "x86_64-pc-windows-msvc\debug", "x86_64-pc-windows-msvc\release") {
    foreach ($sub in "examples", "deps") {
        New-Item -ItemType Directory -Force (Join-Path $targetRoot "$profile\build\$sub") | Out-Null
    }
}
