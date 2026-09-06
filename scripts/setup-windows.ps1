[CmdletBinding()]
param([switch]$Force)

$ErrorActionPreference = 'Stop'
$rootDir = Split-Path -Parent $PSScriptRoot
$toolsDir = Join-Path $rootDir '.tools'
$nodeVersion = '24.20.0'
$nodeArchiveName = "node-v$nodeVersion-win-x64.zip"
$nodeDir = Join-Path $toolsDir 'node'
$llamaDir = Join-Path $toolsDir 'llama.cpp'
$downloadsDir = Join-Path $toolsDir 'downloads'
$cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'

function Download-File([string]$Url, [string]$Destination) {
    Write-Host "Downloading $Url"
    Invoke-WebRequest -Uri $Url -OutFile $Destination -UseBasicParsing
}

function Add-CurrentPath([string]$Path) {
    if ($env:Path -notlike "*$Path*") { $env:Path = "$Path;$env:Path" }
}

New-Item -ItemType Directory -Force $toolsDir, $downloadsDir | Out-Null

if ($Force -or -not (Test-Path (Join-Path $nodeDir 'npm.cmd'))) {
    $archive = Join-Path $downloadsDir $nodeArchiveName
    $expanded = Join-Path $downloadsDir "node-v$nodeVersion-win-x64"
    Download-File "https://nodejs.org/dist/v$nodeVersion/$nodeArchiveName" $archive
    $checksums = Join-Path $downloadsDir 'node-SHASUMS256.txt'
    Download-File "https://nodejs.org/dist/v$nodeVersion/SHASUMS256.txt" $checksums
    $expectedLine = Get-Content $checksums | Where-Object { $_ -match "\s+$([regex]::Escape($nodeArchiveName))$" } | Select-Object -First 1
    if (-not $expectedLine) { throw 'Could not find the Node.js archive checksum.' }
    $expectedHash = ($expectedLine -split '\s+')[0].ToUpperInvariant()
    $actualHash = (Get-FileHash $archive -Algorithm SHA256).Hash
    if ($actualHash -ne $expectedHash) { throw 'Node.js download checksum validation failed.' }
    Remove-Item $expanded -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item $nodeDir -Recurse -Force -ErrorAction SilentlyContinue
    Expand-Archive $archive $downloadsDir -Force
    Move-Item $expanded $nodeDir
    Write-Host "Node.js installed locally in $nodeDir"
}
Add-CurrentPath $nodeDir

$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
$vcToolsFound = $false
if (Test-Path $vswhere) {
    $installation = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    $vcToolsFound = -not [string]::IsNullOrWhiteSpace($installation)
}
if (-not $vcToolsFound) {
    Write-Host 'Microsoft C++ Build Tools are required by Rust on Windows.'
    Write-Host 'Windows may ask for administrator permission. Installation can take several minutes.'
    $installer = Join-Path $downloadsDir 'vs_BuildTools.exe'
    Download-File 'https://aka.ms/vs/17/release/vs_BuildTools.exe' $installer
    $arguments = '--quiet --wait --norestart --nocache --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended'
    $process = Start-Process $installer -ArgumentList $arguments -Verb RunAs -Wait -PassThru
    if ($process.ExitCode -notin 0, 3010) { throw "C++ Build Tools installation failed with exit code $($process.ExitCode)." }
    if ($process.ExitCode -eq 3010) { Write-Warning 'Restart Windows before the first build.' }
}

if ($Force -or -not (Test-Path (Join-Path $cargoBin 'cargo.exe'))) {
    $rustup = Join-Path $downloadsDir 'rustup-init.exe'
    Download-File 'https://win.rustup.rs/x86_64' $rustup
    Write-Host 'Installing the Rust MSVC toolchain for the current user...'
    & $rustup -y --default-toolchain stable --default-host x86_64-pc-windows-msvc --no-modify-path
    if ($LASTEXITCODE -ne 0) { throw "Rust installation failed with exit code $LASTEXITCODE." }
}
Add-CurrentPath $cargoBin

if ($Force -or -not (Test-Path (Join-Path $llamaDir 'llama-server.exe'))) {
    Write-Host 'Finding the newest official llama.cpp Windows CPU build...'
    $headers = @{ 'User-Agent' = 'deepLocal-Windows-Setup' }
    # llama.cpp publishes its regular build-number releases as prereleases.
    # GitHub's /releases/latest endpoint excludes those builds and can point to
    # an unrelated stable release, so inspect the recent release feed instead.
    $architecture = if ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture -eq 'Arm64') { 'arm64' } else { 'x64' }
    $releases = Invoke-RestMethod -Uri 'https://api.github.com/repos/ggml-org/llama.cpp/releases?per_page=10' -Headers $headers
    $llamaAsset = $releases |
        Where-Object { -not $_.draft } |
        ForEach-Object { $_.assets } |
        Where-Object { $_.name -match "^llama-.+-bin-win-cpu-$architecture\.zip$" } |
        Select-Object -First 1
    if (-not $llamaAsset) {
        throw "No recent llama.cpp release contains a Windows $architecture CPU package. Check https://github.com/ggml-org/llama.cpp/releases/."
    }

    $llamaArchive = Join-Path $downloadsDir $llamaAsset.name
    Download-File $llamaAsset.browser_download_url $llamaArchive
    Remove-Item $llamaDir -Recurse -Force -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force $llamaDir | Out-Null
    Expand-Archive $llamaArchive $llamaDir -Force

    $llamaServer = Get-ChildItem $llamaDir -Filter 'llama-server.exe' -File -Recurse | Select-Object -First 1
    if (-not $llamaServer) { throw 'llama.cpp was downloaded, but llama-server.exe was not found in the package.' }
    if ($llamaServer.DirectoryName -ne $llamaDir) {
        Get-ChildItem $llamaServer.DirectoryName -Force | Move-Item -Destination $llamaDir -Force
    }
    Write-Host "llama.cpp $($llamaAsset.name) installed locally in $llamaDir"
}
$env:DEEPLOCAL_LLAMA_SERVER = Join-Path $llamaDir 'llama-server.exe'

Write-Host ''
Write-Host 'Windows dependencies are ready:'
& node.exe --version
& npm.cmd --version
& cargo.exe --version
& $env:DEEPLOCAL_LLAMA_SERVER --version
