<#
.SYNOPSIS
    Signs a Windows binary. Tauri runs this for Joona, the engines and the
    installer when building with tauri.release.conf.json.

.DESCRIPTION
    Uses the first of these that's set up, and leaves the file unsigned
    otherwise (Windows SmartScreen then warns people who download it):

    - Azure Trusted Signing: AZURE_TRUSTED_SIGNING_ENDPOINT, _ACCOUNT and
      _PROFILE, plus AZURE_TENANT_ID, AZURE_CLIENT_ID and AZURE_CLIENT_SECRET.
      Needs trusted-signing-cli (cargo install trusted-signing-cli).
    - A code-signing certificate: WINDOWS_CERTIFICATE (the .pfx file, base64)
      and WINDOWS_CERTIFICATE_PASSWORD. Needs signtool from the Windows SDK.
#>
param([Parameter(Mandatory)][string]$File)

$ErrorActionPreference = "Stop"
$description = "Joona"
$timestamp = "http://timestamp.digicert.com"
$name = Split-Path -Leaf $File

if ($env:AZURE_TRUSTED_SIGNING_ACCOUNT) {
    & trusted-signing-cli -e $env:AZURE_TRUSTED_SIGNING_ENDPOINT -a $env:AZURE_TRUSTED_SIGNING_ACCOUNT `
        -c $env:AZURE_TRUSTED_SIGNING_PROFILE -d $description $File
    if ($LASTEXITCODE -ne 0) { throw "trusted-signing-cli couldn't sign $name (exit code $LASTEXITCODE)." }
}
elseif ($env:WINDOWS_CERTIFICATE) {
    $signtool = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\signtool.exe" |
        Sort-Object FullName -Descending | Select-Object -First 1
    if (-not $signtool) { throw "signtool.exe wasn't found. Install the Windows SDK." }

    $pfx = Join-Path ([IO.Path]::GetTempPath()) "joona-signing-$PID.pfx"
    [IO.File]::WriteAllBytes($pfx, [Convert]::FromBase64String($env:WINDOWS_CERTIFICATE))
    try {
        & $signtool.FullName sign /f $pfx /p $env:WINDOWS_CERTIFICATE_PASSWORD /fd sha256 `
            /tr $timestamp /td sha256 /d $description $File
        if ($LASTEXITCODE -ne 0) { throw "signtool couldn't sign $name (exit code $LASTEXITCODE)." }
    }
    finally {
        Remove-Item $pfx -ErrorAction SilentlyContinue
    }
}
else {
    Write-Host "Not signing ${name}: no signing credentials are set."
}
