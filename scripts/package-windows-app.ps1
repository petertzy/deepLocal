[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$desktop = Join-Path $root 'apps\desktop'
$runtime = Join-Path $desktop 'src-tauri\resources\llama-runtime'
$dist = Join-Path $root 'dist'
$nodeDir = Join-Path $root '.tools\node'
$cargoDir = Join-Path $env:USERPROFILE '.cargo\bin'
$env:Path = "$nodeDir;$cargoDir;$env:Path"

foreach ($command in @('npm.cmd', 'node.exe', 'cargo.exe')) {
    if (-not (Get-Command $command -ErrorAction SilentlyContinue)) {
        throw "Missing $command. Run scripts\setup-windows.ps1 first."
    }
}
$server = if ($env:DEEPLOCAL_LLAMA_SERVER) { $env:DEEPLOCAL_LLAMA_SERVER } elseif ($env:LLAMA_SERVER) { $env:LLAMA_SERVER } else { Join-Path $root '.tools\llama.cpp\llama-server.exe' }
if (-not (Test-Path $server)) { throw 'llama-server.exe is required. Run scripts\setup-windows.ps1 first.' }
$server = (Resolve-Path $server).Path
New-Item -ItemType Directory -Force -Path $runtime | Out-Null
Get-ChildItem $runtime -Force | Remove-Item -Recurse -Force
$sourceDir = Split-Path -Parent $server
Copy-Item (Join-Path $sourceDir '*') $runtime -Recurse -Force
if (-not (Test-Path (Join-Path $runtime 'llama-server.exe'))) { throw 'Failed to bundle llama-server.exe.' }

$configPath = Join-Path ([IO.Path]::GetTempPath()) ("deeplocal-tauri-" + [guid]::NewGuid() + '.json')
try {
    $config = Get-Content (Join-Path $desktop 'src-tauri\tauri.conf.json') -Raw | ConvertFrom-Json
    $config.bundle.targets = @('nsis')
    if ($env:DEEPLOCAL_RELEASE_VERSION) { $config.version = $env:DEEPLOCAL_RELEASE_VERSION.TrimStart('v') }
    if (-not $env:TAURI_SIGNING_PRIVATE_KEY) {
        $config.bundle.createUpdaterArtifacts = $false
        $config.plugins.PSObject.Properties.Remove('updater')
    }
    $config | ConvertTo-Json -Depth 30 | Set-Content $configPath -Encoding UTF8
    Push-Location $desktop
    try {
        if (-not (Test-Path 'node_modules')) {
            & npm.cmd ci
            if ($LASTEXITCODE -ne 0) { throw 'npm ci failed.' }
        }
        & npm.cmd run tauri:build -- --config $configPath
        if ($LASTEXITCODE -ne 0) { throw 'Tauri build failed.' }
    } finally { Pop-Location }
    New-Item -ItemType Directory -Force -Path $dist | Out-Null
    Get-ChildItem (Join-Path $root 'target\release\bundle\nsis') -Filter '*.exe' |
        Copy-Item -Destination $dist
    Write-Host "Windows installer is available in $dist"
} finally {
    Remove-Item $configPath -ErrorAction SilentlyContinue
}
