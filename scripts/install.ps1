#!/usr/bin/env pwsh
# nudge installer script for Windows (PowerShell)
#
# Usage:
#   irm https://raw.githubusercontent.com/attunehq/nudge/main/scripts/install.ps1 | iex
#   $env:Version="0.1.0"; irm https://raw.githubusercontent.com/attunehq/nudge/main/scripts/install.ps1 | iex
#   $env:BinDir="C:\Tools"; irm https://raw.githubusercontent.com/attunehq/nudge/main/scripts/install.ps1 | iex
#
# Options (set via environment variables or script parameters):
#   Version      Specify a version (default: latest)
#   BinDir       Specify the installation directory (default: $env:LOCALAPPDATA\Programs\nudge)
#   Help         Show help message (set $env:Help="true")

param(
    [string]$Version = $env:Version,
    [string]$BinDir = $env:BinDir,
    [switch]$Help = ($env:Help -eq "true")
)

# GitHub repository configuration
$REPO = "attunehq/nudge"
$GITHUB_API = "https://api.github.com/repos/$REPO/releases"
$GITHUB_DOWNLOAD = "https://github.com/$REPO/releases/download"

function Write-Info {
    param([string]$Message)
    Write-Host $Message -ForegroundColor Green
}

function Write-Error-Message {
    param([string]$Message)
    Write-Host "Error: $Message" -ForegroundColor Red
    exit 1
}

function Write-Warning-Message {
    param([string]$Message)
    Write-Host "Warning: $Message" -ForegroundColor Yellow
}

function Get-NormalizedPathEntry {
    param([string]$Entry)

    if ([string]::IsNullOrWhiteSpace($Entry)) {
        return $null
    }

    $expanded = [Environment]::ExpandEnvironmentVariables($Entry.Trim().Trim('"'))
    try {
        $expanded = [System.IO.Path]::GetFullPath($expanded)
    }
    catch {
        # Keep the entry as written; it still compares by text.
    }

    return $expanded.TrimEnd('\', '/')
}

function Test-PathContainsDirectory {
    param(
        [string]$PathValue,
        [string]$Directory
    )

    if ([string]::IsNullOrEmpty($PathValue)) {
        return $false
    }

    $target = Get-NormalizedPathEntry $Directory
    foreach ($entry in $PathValue.Split(';')) {
        $normalized = Get-NormalizedPathEntry $entry
        if ($normalized -and [string]::Equals($normalized, $target, [StringComparison]::OrdinalIgnoreCase)) {
            return $true
        }
    }

    return $false
}

function Show-Help {
    Write-Host @"
nudge installer for Windows

Usage:
  irm https://raw.githubusercontent.com/attunehq/nudge/main/scripts/install.ps1 | iex
  `$env:Version="0.1.0"; irm https://raw.githubusercontent.com/attunehq/nudge/main/scripts/install.ps1 | iex
  `$env:BinDir="C:\Tools"; irm https://raw.githubusercontent.com/attunehq/nudge/main/scripts/install.ps1 | iex

Options (set via environment variables):
  Version      Specify a version (default: latest)
  BinDir       Specify the installation directory (default: `$env:LOCALAPPDATA\Programs\nudge)
  Help         Show this help message (set `$env:Help="true")

Examples:
  # Install latest version
  irm https://raw.githubusercontent.com/attunehq/nudge/main/scripts/install.ps1 | iex

  # Install specific version
  `$env:Version="0.1.0"; irm https://raw.githubusercontent.com/attunehq/nudge/main/scripts/install.ps1 | iex

  # Install to custom directory
  `$env:BinDir="C:\Tools"; irm https://raw.githubusercontent.com/attunehq/nudge/main/scripts/install.ps1 | iex
"@
    exit 0
}

function Get-LatestVersion {
    $latestUrl = "$GITHUB_API/latest"

    try {
        $response = Invoke-RestMethod -Uri $latestUrl -ErrorAction Stop
        return $response.tag_name -replace '^v', ''
    }
    catch {
        Write-Error-Message "Failed to fetch latest release from $latestUrl. Error: $_"
    }
}

function Get-Platform {
    $arch = $env:PROCESSOR_ARCHITECTURE

    switch ($arch) {
        "AMD64" { return "x86_64-pc-windows-gnu" }
        "ARM64" { Write-Error-Message "Windows ARM64 is not currently supported. Please build from source or use x64 emulation." }
        default { Write-Error-Message "Unsupported architecture: $arch" }
    }
}

function Install-Binary {
    param(
        [string]$Platform,
        [string]$Version,
        [string]$InstallDir,
        [string]$TempDir
    )

    $Version = $Version -replace '^v', ''
    $archiveName = "nudge-$Platform.tar.gz"
    $tag = "v$Version"
    $downloadUrl = "$GITHUB_DOWNLOAD/$tag/$archiveName"
    $checksumsUrl = "$GITHUB_DOWNLOAD/$tag/checksums.txt"

    Write-Info "Downloading nudge $Version for $Platform..."

    # Create temporary directory
    $tempExtractDir = Join-Path $TempDir "nudge-install-$(Get-Random)"
    New-Item -ItemType Directory -Force -Path $tempExtractDir | Out-Null

    $archivePath = Join-Path $tempExtractDir $archiveName

    try {
        # Download archive
        Invoke-WebRequest -Uri $downloadUrl -OutFile $archivePath -ErrorAction Stop
    }
    catch {
        Write-Error-Message "Failed to download from $downloadUrl. Error: $_"
    }

    Write-Info "Verifying checksum..."

    # Download checksums
    try {
        $checksums = Invoke-RestMethod -Uri $checksumsUrl -ErrorAction Stop
    }
    catch {
        Write-Error-Message "Could not download checksums from $checksumsUrl. Error: $_"
    }

    # Calculate hash
    try {
        $hash = (Get-FileHash -Path $archivePath -Algorithm SHA256 -ErrorAction Stop).Hash.ToLower()
    }
    catch {
        Write-Error-Message "Could not calculate checksum for $archivePath. Error: $_"
    }

    # Find expected hash
    $expectedHash = ($checksums -split "`n" | Where-Object { $_ -match $archiveName } | ForEach-Object {
        ($_ -split '\s+')[0]
    } | Select-Object -First 1)

    if ([string]::IsNullOrWhiteSpace($expectedHash)) {
        Write-Error-Message "Could not find checksum for $archiveName in $checksumsUrl"
    }

    if ($hash -ne $expectedHash.ToLower()) {
        Write-Error-Message "Checksum verification failed!`nExpected: $expectedHash`nGot: $hash"
    }

    Write-Info "Checksum verified successfully"

    Write-Info "Extracting archive..."

    # Extract archive
    try {
        # Check if tar is available (Windows 10+)
        if (Get-Command tar -ErrorAction SilentlyContinue) {
            tar -xzf $archivePath -C $tempExtractDir
        }
        else {
            Write-Error-Message "tar command not found. Please install tar or upgrade to Windows 10+"
        }
    }
    catch {
        Write-Error-Message "Failed to extract archive: $_"
    }

    # Create installation directory
    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null

    # Find and copy binary
    $extractedDir = Join-Path $tempExtractDir "nudge-$Platform"
    $binaryPath = Join-Path $extractedDir "nudge.exe"
    $targetPath = Join-Path $InstallDir "nudge.exe"

    if (-not (Test-Path $binaryPath)) {
        Write-Error-Message "Binary not found in archive at $binaryPath"
    }

    Copy-Item -Force $binaryPath $targetPath

    # Cleanup
    Remove-Item -Recurse -Force $tempExtractDir

    # Compare PATH entries one by one, so a directory that only shares a prefix
    # with another entry (C:\Tools vs C:\Tools2) does not count as present.
    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if (-not (Test-PathContainsDirectory $userPath $InstallDir)) {
        Write-Info "Adding $InstallDir to your user PATH..."

        if ([string]::IsNullOrEmpty($userPath)) {
            $newPath = $InstallDir
        }
        else {
            $newPath = $userPath.TrimEnd(';') + ";$InstallDir"
        }

        # Setting a User variable also broadcasts WM_SETTINGCHANGE, so Explorer
        # and the Start menu hand the new PATH to programs they launch from now on.
        [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
        Write-Info "Added to PATH"
    }

    # Programs that were already running keep the PATH they started with. If
    # this session's PATH lacked the install directory, the terminals, editors,
    # and launchers that were open before the install most likely lack it too.
    $sessionHadInstallDir = Test-PathContainsDirectory $env:Path $InstallDir
    if (-not $sessionHadInstallDir) {
        $env:Path = "$env:Path".TrimEnd(';') + ";$InstallDir"
    }

    # Display version
    $installedVersion = & $targetPath --version 2>$null
    if ($LASTEXITCODE -eq 0) {
        Write-Info "Installed '$installedVersion' to '$targetPath'"
    }
    else {
        Write-Info "Installed to '$targetPath'"
    }

    # Check what `nudge` resolves to now, so a stale or shadowed install is
    # reported here instead of surprising the user later.
    $resolved = Get-Command nudge -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $resolved) {
        Write-Host ""
        Write-Warning-Message "'nudge' does not resolve in this session. Run it by full path: & '$targetPath'"
    }
    elseif (-not [string]::Equals((Get-NormalizedPathEntry $resolved.Source), (Get-NormalizedPathEntry $targetPath), [StringComparison]::OrdinalIgnoreCase)) {
        Write-Host ""
        Write-Warning-Message "'nudge' resolves to '$($resolved.Source)', which comes before '$InstallDir' on PATH. Remove that copy, or move '$InstallDir' earlier in PATH, to use the version you just installed."
    }

    if (-not $sessionHadInstallDir) {
        Write-Host ""
        Write-Warning-Message "Terminals, editors, and launchers that were already open keep their old PATH, so 'nudge' may not be found in them until you restart them. New terminals started from the Start menu or Explorer will find it. To use it in another open PowerShell window now, run: `$env:Path += ';$InstallDir'"
    }

    Write-Host ""
    Write-Info "Installation complete!"
    Write-Host ""
    Write-Host "Run 'nudge --help' to get started"
}

# Main execution
function Main {
    if ($Help) {
        Show-Help
    }

    # Set default bin directory
    if ([string]::IsNullOrEmpty($BinDir)) {
        $BinDir = Join-Path $env:LOCALAPPDATA "Programs\nudge"
    }

    # Set default temp directory
    $TempDir = $env:TEMP

    # Detect platform
    $PLATFORM = Get-Platform
    Write-Info "Detected platform: $PLATFORM"

    # Get version
    if ([string]::IsNullOrEmpty($Version)) {
        $Version = Get-LatestVersion
        Write-Info "Installing latest version: $Version"
    }
    else {
        Write-Info "Installing version: $Version"
    }

    # Install
    Install-Binary -Platform $PLATFORM -Version $Version -InstallDir $BinDir -TempDir $TempDir
}

Main
