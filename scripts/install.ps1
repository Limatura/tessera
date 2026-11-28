# Installs `tess`, the Tessera command-line client, from a GitHub Release.
#
#   irm https://raw.githubusercontent.com/Limatura/tessera/main/scripts/install.ps1 | iex
#   & ([scriptblock]::Create((irm .../install.ps1))) -Version 0.1.0 -Dir C:\tools
#
# Without -Version it installs the latest release that is not a pre-release.
# The archive is checked against the release's SHA256SUMS before anything is
# installed, and the install directory is added to your user PATH.
# Windows x86_64; on Linux and macOS, use install.sh.
[CmdletBinding()]
param(
    [string]$Version = $env:TESS_VERSION,
    [string]$Dir = $(if ($env:TESS_INSTALL_DIR) { $env:TESS_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'Programs\tess' })
)

$ErrorActionPreference = 'Stop'
# Invoke-WebRequest draws a progress bar that makes downloads many times slower
# in Windows PowerShell 5.1.
$ProgressPreference = 'SilentlyContinue'
# Windows PowerShell 5.1 negotiates TLS 1.0 unless told otherwise; GitHub needs 1.2.
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

function Fail($Message) {
    throw "install.ps1: $Message"
}

$repo = if ($env:TESS_REPO) { $env:TESS_REPO } else { 'Limatura/tessera' }

if ($env:OS -ne 'Windows_NT') {
    Fail 'this script is for Windows; use install.sh on Linux and macOS'
}
# PROCESSOR_ARCHITEW6432 is set when a 32-bit shell runs on 64-bit Windows.
$arch = if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 } else { $env:PROCESSOR_ARCHITECTURE }
if ($arch -ne 'AMD64') {
    Fail "no build for $arch; download tess from https://github.com/$repo/releases"
}
$archive = 'tess-windows-x86_64.zip'

if ($env:TESS_DOWNLOAD_BASE) {
    $base = $env:TESS_DOWNLOAD_BASE
} elseif ($Version) {
    $base = "https://github.com/$repo/releases/download/v$($Version.TrimStart('v'))"
} else {
    $base = "https://github.com/$repo/releases/latest/download"
}

$tmp = Join-Path ([IO.Path]::GetTempPath()) ([IO.Path]::GetRandomFileName())
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
    try {
        Invoke-WebRequest -UseBasicParsing -Uri "$base/$archive" -OutFile (Join-Path $tmp $archive)
    } catch {
        if (-not $Version) {
            Fail "could not download $archive; if there is no stable release yet, pass -Version"
        }
        Fail "could not download $archive from $base"
    }
    try {
        Invoke-WebRequest -UseBasicParsing -Uri "$base/SHA256SUMS" -OutFile (Join-Path $tmp 'SHA256SUMS')
    } catch {
        Fail "could not download SHA256SUMS from $base"
    }

    # `<hash>  <name>`, as sha256sum writes it.
    $pattern = "^([0-9a-fA-F]{64})\s+\*?$([regex]::Escape($archive))$"
    $line = Get-Content (Join-Path $tmp 'SHA256SUMS') | Where-Object { $_ -match $pattern } | Select-Object -First 1
    if (-not $line) { Fail "SHA256SUMS has no line for $archive" }
    $expected = [regex]::Match($line, $pattern).Groups[1].Value
    $actual = (Get-FileHash -Algorithm SHA256 -Path (Join-Path $tmp $archive)).Hash
    if ($actual -ne $expected) { Fail "$archive does not match its checksum" }

    $unpacked = Join-Path $tmp 'unpacked'
    Expand-Archive -Path (Join-Path $tmp $archive) -DestinationPath $unpacked
    if (-not (Test-Path (Join-Path $unpacked 'tess.exe'))) { Fail "$archive has no tess.exe in it" }

    New-Item -ItemType Directory -Force -Path $Dir | Out-Null
    $target = Join-Path $Dir 'tess.exe'
    # Copied beside the target and renamed into place, so a tess.exe already
    # there is swapped whole, never half written over. A running tess.exe can be
    # renamed over but not overwritten, which is why the plain Copy-Item is not
    # enough.
    $staged = Join-Path $Dir 'tess.exe.tmp'
    Copy-Item -Force (Join-Path $unpacked 'tess.exe') $staged
    Move-Item -Force $staged $target
} finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}

Write-Host "Installed $(& $target --version) to $target"

# The user's own PATH, read from the registry: $env:Path in this session also
# holds the machine PATH, and writing that back would copy it into the user's.
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
$entries = if ($userPath) { $userPath -split ';' | Where-Object { $_ } } else { @() }
$resolved = [IO.Path]::GetFullPath($Dir).TrimEnd('\')
$onPath = $entries | Where-Object { [IO.Path]::GetFullPath($_).TrimEnd('\') -ieq $resolved }
if (-not $onPath) {
    [Environment]::SetEnvironmentVariable('Path', (($entries + $Dir) -join ';'), 'User')
    Write-Host "Added $Dir to your PATH. Open a new terminal to use tess."
}
# This session too, so `tess` works right away.
if (($env:Path -split ';') -notcontains $Dir) {
    $env:Path = "$Dir;$env:Path"
}
