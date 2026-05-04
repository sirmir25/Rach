# Rach installer for Windows (PowerShell 5+).
#
# Downloads a pre-built binary from GitHub Releases — no Rust toolchain required.
#
# One-liner:
#   iwr -useb https://raw.githubusercontent.com/sirmir25/Rach/main/installers/install.ps1 | iex
#
# From a checkout:
#   powershell -ExecutionPolicy Bypass -File installers\install.ps1 [-InstallDir <path>]
#
# Env:
#   RACH_VERSION   pin a release tag (default: latest)
#   RACH_REPO      override repo (default sirmir25/Rach)

[CmdletBinding()]
param(
    [string]$InstallDir = $env:RACH_INSTALL_DIR
)

$ErrorActionPreference = 'Stop'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$repo    = if ($env:RACH_REPO)    { $env:RACH_REPO }    else { 'sirmir25/Rach' }
$version = if ($env:RACH_VERSION) { $env:RACH_VERSION } else { 'latest' }

if (-not $InstallDir) {
    $InstallDir = Join-Path $env:LOCALAPPDATA 'Programs\rach'
}

# Detect arch
$proc = $env:PROCESSOR_ARCHITECTURE
if ($env:PROCESSOR_ARCHITEW6432) { $proc = $env:PROCESSOR_ARCHITEW6432 }
switch ($proc) {
    'AMD64' { $arch = 'x86_64' }
    'ARM64' {
        Write-Host '!! No native ARM64 build yet — installing the x86_64 build (runs under emulation).' -ForegroundColor Yellow
        $arch = 'x86_64'
    }
    default { throw "Unsupported processor architecture: $proc" }
}

$target = "$arch-pc-windows-msvc"
$asset  = "rach-$target.zip"
$url = if ($version -eq 'latest') {
    "https://github.com/$repo/releases/latest/download/$asset"
} else {
    "https://github.com/$repo/releases/download/$version/$asset"
}

$tmp = New-Item -ItemType Directory -Path (Join-Path $env:TEMP "rach-install-$([System.Guid]::NewGuid().ToString('N'))")
try {
    $zip = Join-Path $tmp $asset
    Write-Host "==> Downloading $asset" -ForegroundColor Cyan
    try {
        Invoke-WebRequest -Uri $url -OutFile $zip -UseBasicParsing
    } catch {
        throw "Download failed: $url`n$($_.Exception.Message)"
    }

    Write-Host "==> Extracting" -ForegroundColor Cyan
    Expand-Archive -Path $zip -DestinationPath $tmp -Force

    $exe = Get-ChildItem -Path $tmp -Recurse -Filter 'rach.exe' | Select-Object -First 1
    if (-not $exe) { throw 'archive missing rach.exe' }

    if (-not (Test-Path $InstallDir)) {
        New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    }
    $dst = Join-Path $InstallDir 'rach.exe'
    Write-Host "==> Installing to $dst" -ForegroundColor Cyan
    Copy-Item $exe.FullName $dst -Force

    Write-Host '==> Verifying' -ForegroundColor Cyan
    & $dst version

    # Add InstallDir to user PATH if missing
    $userPath = [Environment]::GetEnvironmentVariable('PATH', 'User')
    $segments = @()
    if ($userPath) { $segments = $userPath -split ';' | Where-Object { $_ -ne '' } }
    if (-not ($segments -contains $InstallDir)) {
        $newPath = if ($userPath) { "$userPath;$InstallDir" } else { $InstallDir }
        [Environment]::SetEnvironmentVariable('PATH', $newPath, 'User')
        Write-Host "==> Added $InstallDir to user PATH (open a new shell to pick it up)" -ForegroundColor Cyan
    }

    Write-Host 'Installed. Try:  rach examples\hello.rach' -ForegroundColor Green
}
finally {
    Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
}
