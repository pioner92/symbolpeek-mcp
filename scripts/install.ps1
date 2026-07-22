$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$Repository = if ($env:SYMBOLPEEK_REPOSITORY) {
    $env:SYMBOLPEEK_REPOSITORY
} else {
    "pioner92/symbolpeek-mcp"
}
$InstallDir = if ($env:SYMBOLPEEK_INSTALL_DIR) {
    $env:SYMBOLPEEK_INSTALL_DIR
} else {
    Join-Path $env:LOCALAPPDATA "SymbolPeek"
}

if ($env:PROCESSOR_ARCHITECTURE -ne "AMD64") {
    throw "Only Windows x86-64 is currently available. Download another platform package manually from GitHub Releases."
}

$Target = "x86_64-pc-windows-msvc"
$Package = "symbolpeek-$Target"
$ArchiveName = "$Package.zip"
$BaseUrl = if ($env:SYMBOLPEEK_DOWNLOAD_BASE_URL) {
    $env:SYMBOLPEEK_DOWNLOAD_BASE_URL
} else {
    "https://github.com/$Repository/releases/latest/download"
}
$TempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("symbolpeek-install-" + [guid]::NewGuid())

try {
    New-Item -ItemType Directory -Force $TempDir | Out-Null
    $Archive = Join-Path $TempDir $ArchiveName
    $Checksum = "$Archive.sha256"

    Write-Host "Downloading SymbolPeek for Windows x86-64..."
    Invoke-WebRequest "$BaseUrl/$ArchiveName" -OutFile $Archive
    Invoke-WebRequest "$BaseUrl/$ArchiveName.sha256" -OutFile $Checksum

    $Expected = ((Get-Content $Checksum) -split "\s+")[0].ToLowerInvariant()
    $Actual = (Get-FileHash -Algorithm SHA256 $Archive).Hash.ToLowerInvariant()
    if ($Actual -ne $Expected) {
        throw "Checksum verification failed for $ArchiveName"
    }

    Expand-Archive $Archive -DestinationPath $TempDir -Force
    $PackageDir = Join-Path $TempDir $Package
    New-Item -ItemType Directory -Force $InstallDir | Out-Null
    Copy-Item -Recurse -Force (Join-Path $PackageDir "*") $InstallDir

    $UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
    $PathEntries = @($UserPath -split ";" | Where-Object { $_ })
    if ($PathEntries -notcontains $InstallDir) {
        $UpdatedPath = if ($UserPath) { "$UserPath;$InstallDir" } else { $InstallDir }
        [Environment]::SetEnvironmentVariable("Path", $UpdatedPath, "User")
    }
    if (($env:Path -split ";") -notcontains $InstallDir) {
        $env:Path = "$InstallDir;$env:Path"
    }

    $Binary = Join-Path $InstallDir "symbolpeek.exe"
    & $Binary --version
    Write-Host "Installed in: $InstallDir"
    if (-not (Get-Command node -ErrorAction SilentlyContinue)) {
        Write-Host "Note: install Node.js 20+ to enable TypeScript and JavaScript operations."
    }
    Write-Host ""
    Write-Host "Connect to Codex:"
    Write-Host "  codex mcp add symbolpeek -- `"$Binary`""
    Write-Host "Connect to Claude Code:"
    Write-Host "  claude mcp add --transport stdio --scope user symbolpeek -- `"$Binary`""
} finally {
    if (Test-Path $TempDir) {
        Remove-Item -Recurse -Force $TempDir
    }
}
