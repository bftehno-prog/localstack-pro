$ErrorActionPreference = "Stop"

function Find-SignTool {
  $command = Get-Command signtool.exe -ErrorAction SilentlyContinue
  if ($command) {
    return $command.Source
  }

  $sdkRoot = "C:\Program Files (x86)\Windows Kits\10\bin"
  $candidate = Get-ChildItem -LiteralPath $sdkRoot -Recurse -Filter signtool.exe -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -match "\\x64\\signtool\.exe$" } |
    Sort-Object FullName -Descending |
    Select-Object -First 1

  if ($candidate) {
    return $candidate.FullName
  }

  return $null
}

$signTool = Find-SignTool
if (-not $signTool) {
  $winget = Get-Command winget.exe -ErrorAction SilentlyContinue
  if (-not $winget) {
    throw "winget.exe was not found. Install Windows SDK manually and select Windows SDK Signing Tools for Desktop Apps."
  }

  winget install --id Microsoft.WindowsSDK.10.0.26100 -e --accept-package-agreements --accept-source-agreements
  $signTool = Find-SignTool
}

if (-not $signTool) {
  throw "signtool.exe was not found after Windows SDK installation."
}

$signToolDir = Split-Path -Parent $signTool
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($userPath -notlike "*$signToolDir*") {
  [Environment]::SetEnvironmentVariable("Path", ($userPath.TrimEnd(";") + ";" + $signToolDir), "User")
}

Write-Host "signtool.exe ready: $signTool"
