<#
.SYNOPSIS
    Installs what you need to build Joona on Windows, then builds the engine once.

.DESCRIPTION
    - Rust (MSVC toolchain) via rustup, installed per user.
    - Visual Studio Build Tools with the C++ workload, if no MSVC toolchain is present.
    - CMake, if neither a standalone CMake nor the one bundled with Visual Studio is found.
    - libclang for bindgen. If LLVM isn't installed, the official libclang wheel from PyPI is
      unpacked into %LOCALAPPDATA%\joona-dev\libclang, which needs no admin rights.
    - npm dependencies.
    Installs that need admin rights go through winget and will show a UAC prompt.
#>
param(
    [switch]$SkipEngine
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot

function Test-Command($name) {
    return [bool](Get-Command $name -ErrorAction SilentlyContinue)
}

function Install-WithWinget($id, $extra = @()) {
    if (-not (Test-Command winget)) {
        throw "winget is not available. Install '$id' manually."
    }
    Write-Host "Installing $id with winget..."
    winget install --id $id --exact --accept-source-agreements --accept-package-agreements @extra
}

# Rust
$cargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
if (-not (Test-Path (Join-Path $cargoBin "cargo.exe"))) {
    Write-Host "Installing Rust (rustup, MSVC toolchain)..."
    $rustup = Join-Path $env:TEMP "rustup-init.exe"
    Invoke-WebRequest "https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe" -OutFile $rustup
    & $rustup -y --default-toolchain stable --default-host x86_64-pc-windows-msvc --profile minimal
}
$env:Path = "$cargoBin;$env:Path"

# MSVC C++ toolchain
$vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
$vs = $null
if (Test-Path $vswhere) {
    $vs = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
}
if (-not $vs) {
    Install-WithWinget "Microsoft.VisualStudio.2022.BuildTools" @("--override", "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended")
}

# CMake (Visual Studio ships one; dev-env.ps1 puts it on PATH)
. (Join-Path $PSScriptRoot "dev-env.ps1")
if (-not (Test-Command cmake)) {
    Install-WithWinget "Kitware.CMake"
}

# libclang (bindgen)
if (-not $env:LIBCLANG_PATH) {
    $dest = Join-Path $env:LOCALAPPDATA "joona-dev\libclang"
    Write-Host "Fetching libclang from the PyPI libclang wheel into $dest..."
    $meta = Invoke-RestMethod "https://pypi.org/pypi/libclang/json"
    $wheel = $meta.urls | Where-Object { $_.filename -like "*win_amd64.whl" } | Select-Object -First 1
    $zip = Join-Path $env:TEMP "libclang-wheel.zip"
    Invoke-WebRequest $wheel.url -OutFile $zip
    $hash = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLower()
    if ($hash -ne $wheel.digests.sha256) { throw "libclang wheel hash mismatch" }
    $unpacked = Join-Path $env:TEMP "libclang-wheel"
    Remove-Item -Recurse -Force $unpacked -ErrorAction SilentlyContinue
    Expand-Archive $zip -DestinationPath $unpacked
    New-Item -ItemType Directory -Force $dest | Out-Null
    Get-ChildItem $unpacked -Recurse -Filter libclang.dll | Copy-Item -Destination $dest -Force
    $env:LIBCLANG_PATH = $dest
}

# Node dependencies
if (-not (Test-Command npm)) {
    throw "Node.js 22 or newer is required: winget install OpenJS.NodeJS.LTS"
}
Push-Location $root
try {
    npm install
}
finally {
    Pop-Location
}

if (-not $SkipEngine) {
    & (Join-Path $PSScriptRoot "build-engine.ps1")
}

Write-Host ""
Write-Host "Done. Start Joona in development mode with: npm run dev"
