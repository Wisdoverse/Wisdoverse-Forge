# Wisdoverse Forge — one-command local agent join (Windows PowerShell).
#
# Rendered and served by the control plane at
#   GET /api/v1/agents/local-join/script.ps1
# Usage (shown in the Create Agent dialog):
#   $env:AGENTFORGE_JOIN_CODE = 'afj_...'; irm <server>/api/v1/agents/local-join/script.ps1 | iex
$ErrorActionPreference = 'Stop'

$ServerUrl = '__AGENTFORGE_SERVER_URL__'
$BinaryBaseUrl = '__AGENTFORGE_BINARY_BASE_URL__'
$JoinCode = $env:AGENTFORGE_JOIN_CODE

if ([string]::IsNullOrWhiteSpace($JoinCode)) {
    Write-Error "Missing pairing code. Re-copy the full join command from the Create Agent dialog."
    exit 2
}

# --- 1. Locate or download the sidecar -------------------------------------
$BinDir = Join-Path $env:USERPROFILE '.agentforge\bin'
$Sidecar = (Get-Command 'agentforge-sidecar' -ErrorAction SilentlyContinue).Source
if (-not $Sidecar) {
    $Candidate = Join-Path $BinDir 'agentforge-sidecar.exe'
    if (Test-Path $Candidate) { $Sidecar = $Candidate }
}
if (-not $Sidecar) {
    $Arch = if ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture -eq 'Arm64') { 'arm64' } else { 'amd64' }
    if ($Arch -eq 'arm64') {
        Write-Error "Windows ARM64 binaries are not published yet. Use the manual setup shown in the Create Agent dialog."
        exit 1
    }
    $Asset = "agentforge-sidecar-windows-$Arch.exe"
    Write-Host "Downloading $Asset ..."
    New-Item -ItemType Directory -Force -Path $BinDir | Out-Null
    $Sidecar = Join-Path $BinDir 'agentforge-sidecar.exe'
    Invoke-WebRequest -Uri "$BinaryBaseUrl/$Asset" -OutFile $Sidecar -UseBasicParsing
    Write-Host "Downloaded to $Sidecar."
    Write-Host "Tip: you can verify release binaries with 'agentforge verify' (see the Host CLI runbook)."
}

# --- 2. Exchange the pairing code for this agent's environment -------------
try {
    $Claim = Invoke-RestMethod -Method Post -Uri "$ServerUrl/api/v1/agents/local-join/claim" `
        -ContentType 'application/json' `
        -Body (@{ code = $JoinCode; format = 'json' } | ConvertTo-Json)
} catch {
    Write-Error "Pairing code rejected. Codes expire after 15 minutes - create the agent again in the dialog to get a fresh command."
    exit 1
}

$EnvDir = Join-Path $env:USERPROFILE '.agentforge\agents'
New-Item -ItemType Directory -Force -Path $EnvDir | Out-Null
foreach ($Pair in $Claim.env.PSObject.Properties) {
    Set-Item -Path "Env:$($Pair.Name)" -Value $Pair.Value
}
$AgentId = if ($Claim.agentId) { $Claim.agentId } else { 'agent' }
$EnvFile = Join-Path $EnvDir "$AgentId.ps1"
($Claim.env.PSObject.Properties | ForEach-Object { "`$env:$($_.Name) = '" + ($_.Value -replace "'", "''") + "'" }) -join "`r`n" |
    Set-Content -Path $EnvFile -Encoding UTF8

# --- 3. Friendly preflight ---------------------------------------------------
$Tool = $Claim.cliTool
if ($Tool -and -not (Get-Command $Tool -ErrorAction SilentlyContinue)) {
    Write-Warning "'$Tool' is not installed on this machine. Install it before sending tasks to this agent."
}

Write-Host ""
Write-Host "Agent connected. Leave this window open while the agent is in use; press Ctrl+C to disconnect."
Write-Host "Environment saved to $EnvFile - reconnect later by running that file, then agentforge-sidecar."
Write-Host ""
& $Sidecar
