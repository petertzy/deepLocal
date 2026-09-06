[CmdletBinding()]
param([switch]$Restart, [switch]$Stop, [switch]$Build, [switch]$Help)

$ErrorActionPreference = 'Stop'
$rootDir = Split-Path -Parent $PSScriptRoot
$desktopDir = Join-Path $rootDir 'apps\desktop'
$backendPort = if ($env:DEEPLOCAL_BACKEND_PORT) { $env:DEEPLOCAL_BACKEND_PORT } else { '14567' }
$frontendPort = if ($env:DEEPLOCAL_FRONTEND_PORT) { $env:DEEPLOCAL_FRONTEND_PORT } else { '5173' }
$backendProcess = $null

function Initialize-Dependencies {
    $localNode = Join-Path $rootDir '.tools\node'
    $localLlama = Join-Path $rootDir '.tools\llama.cpp\llama-server.exe'
    $cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
    if (Test-Path (Join-Path $localNode 'npm.cmd')) { $env:Path = "$localNode;$env:Path" }
    if (Test-Path (Join-Path $cargoBin 'cargo.exe')) { $env:Path = "$cargoBin;$env:Path" }
    if (Test-Path $localLlama) { $env:DEEPLOCAL_LLAMA_SERVER = $localLlama }
    if (-not (Get-Command npm.cmd -ErrorAction SilentlyContinue) -or
        -not (Get-Command cargo.exe -ErrorAction SilentlyContinue) -or
        -not ($env:DEEPLOCAL_LLAMA_SERVER -or $env:DEELOCAL_LLAMA_SERVER -or $env:LLAMA_SERVER)) {
        Write-Host 'Required Windows development tools are missing.'
        Write-Host 'Starting the one-time automatic setup...'
        & (Join-Path $PSScriptRoot 'setup-windows.ps1')
        $env:Path = "$localNode;$cargoBin;$env:Path"
        $env:DEEPLOCAL_LLAMA_SERVER = $localLlama
    }
}

function Require-Command([string]$Name) {
    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        throw "Missing required command: $Name. Install it, reopen PowerShell, and try again."
    }
}

function Get-PortProcessIds([int]$Port) {
    @(Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue |
        Select-Object -ExpandProperty OwningProcess -Unique)
}

function Test-Service([string]$Url) {
    try { Invoke-WebRequest $Url -UseBasicParsing -TimeoutSec 2 | Out-Null; $true }
    catch { $false }
}

function Stop-Port([int]$Port) {
    $processIds = Get-PortProcessIds $Port
    foreach ($processId in $processIds) {
        Write-Host "Stopping process $processId on port $Port..."
        Stop-Process -Id $processId -Force -ErrorAction SilentlyContinue
    }
    if ($processIds.Count) { Start-Sleep 1 }
}

if ($Help) {
    Write-Host 'Usage: .\scripts\start-dev.ps1 [-Restart|-Stop|-Build]'
    exit 0
}

if ($Stop) {
    Stop-Port $frontendPort
    Stop-Port $backendPort
    Write-Host 'deepLocal servers are stopped.'
    exit 0
}

Initialize-Dependencies
Require-Command npm.cmd
if ($Build) {
    Push-Location $desktopDir
    try {
        if (-not (Test-Path node_modules)) {
            & npm.cmd install
            if ($LASTEXITCODE) { throw 'npm install failed.' }
        }
        & npm.cmd run build
        if ($LASTEXITCODE) { throw 'Frontend build failed.' }
    } finally { Pop-Location }
    exit 0
}

Require-Command cargo.exe
if (-not ($env:DEEPLOCAL_LLAMA_SERVER -or $env:LLAMA_SERVER)) {
    $llama = Get-Command llama-server.exe -ErrorAction SilentlyContinue
    if ($llama) { $env:DEEPLOCAL_LLAMA_SERVER = $llama.Source }
    else { Write-Warning 'llama-server was not found. The UI can start, but GGUF chat requires llama.cpp.' }
}

if ($Restart) { Stop-Port $frontendPort; Stop-Port $backendPort }

$frontendRunning = (Get-PortProcessIds $frontendPort).Count -gt 0
$backendRunning = (Get-PortProcessIds $backendPort).Count -gt 0
if ($frontendRunning -and -not (Test-Service "http://127.0.0.1:$frontendPort/")) {
    throw "Port $frontendPort is occupied. Use -Restart."
}
if ($backendRunning -and -not (Test-Service "http://127.0.0.1:$backendPort/health")) {
    throw "Port $backendPort is occupied. Use -Restart."
}

try {
    if (-not $backendRunning) {
        Write-Host "Starting backend on http://127.0.0.1:$backendPort ..."
        $backendProcess = Start-Process cargo.exe -ArgumentList @('run','-p','deeplocal','--','serve','--port',$backendPort) -WorkingDirectory $rootDir -NoNewWindow -PassThru
    }

    Write-Host 'Waiting for backend...'
    $ready = $false
    foreach ($attempt in 1..60) {
        if (Test-Service "http://127.0.0.1:$backendPort/health") { $ready = $true; break }
        if ($backendProcess -and $backendProcess.HasExited) {
            throw "Backend exited with code $($backendProcess.ExitCode)."
        }
        Start-Sleep 1
    }
    if (-not $ready) { throw 'Backend did not become ready within 60 seconds.' }

    if ($frontendRunning) {
        Write-Host "Open: http://127.0.0.1:$frontendPort/"
        if ($backendProcess) { $backendProcess.WaitForExit() }
        exit 0
    }

    Push-Location $desktopDir
    try {
        if (-not (Test-Path node_modules)) {
            & npm.cmd install
            if ($LASTEXITCODE) { throw 'npm install failed.' }
        }
        Write-Host "Starting UI. Open: http://127.0.0.1:$frontendPort/"
        & npm.cmd run dev -- --host 127.0.0.1 --port $frontendPort
        if ($LASTEXITCODE) { throw 'Frontend server failed.' }
    } finally { Pop-Location }
} finally {
    if ($backendProcess -and -not $backendProcess.HasExited) {
        Stop-Process $backendProcess.Id -Force
    }
}
