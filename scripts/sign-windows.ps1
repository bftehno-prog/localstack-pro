param(
  [string[]]$Path,
  [string]$PfxPath = $env:LOCALSTACK_SIGN_PFX,
  [string]$PfxPassword = $env:LOCALSTACK_SIGN_PASSWORD,
  [string]$Thumbprint = $env:LOCALSTACK_SIGN_THUMBPRINT,
  [string]$CertificateSubject = $(if ($env:LOCALSTACK_SIGN_SUBJECT) { $env:LOCALSTACK_SIGN_SUBJECT } else { "Farid Leonov LocalStack Pro Dev Signing" }),
  [string]$TimestampServer = $(if ($env:LOCALSTACK_SIGN_TIMESTAMP) { $env:LOCALSTACK_SIGN_TIMESTAMP } else { "http://timestamp.digicert.com" }),
  [string]$SignToolPath = $env:LOCALSTACK_SIGNTOOL
)

$ErrorActionPreference = "Stop"
$tauriConfig = Get-Content -LiteralPath "src-tauri\tauri.conf.json" -Raw | ConvertFrom-Json
$releaseVersion = $tauriConfig.version
$installerFileName = "LocalStack Pro_${releaseVersion}_x64-setup.exe"

function Find-SignTool {
  if ($SignToolPath -and (Test-Path -LiteralPath $SignToolPath)) {
    return (Resolve-Path -LiteralPath $SignToolPath).Path
  }

  $fromPath = Get-Command signtool.exe -ErrorAction SilentlyContinue
  if ($fromPath -and (Test-Path -LiteralPath $fromPath.Source)) {
    return $fromPath.Source
  }

  $sdkRoot = "C:\Program Files (x86)\Windows Kits\10\bin"
  $candidate = Get-ChildItem -LiteralPath $sdkRoot -Recurse -Filter signtool.exe -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -match "\\x64\\signtool\.exe$" } |
    Sort-Object FullName -Descending |
    Select-Object -First 1

  if ($candidate) {
    return $candidate.FullName
  }

  throw "signtool.exe was not found. Install Windows SDK Signing Tools and rerun this script."
}

function Resolve-TargetFiles {
  if ($Path -and $Path.Count -gt 0) {
    return $Path | ForEach-Object { (Resolve-Path -LiteralPath $_).Path }
  }

  $targets = @(
    "src-tauri\target\release\localstack-pro.exe",
    (Join-Path "src-tauri\target\release\bundle\nsis" $installerFileName)
  )

  return $targets | Where-Object { Test-Path -LiteralPath $_ } | ForEach-Object { (Resolve-Path -LiteralPath $_).Path }
}

function Invoke-SignTool {
  param(
    [string]$SignTool,
    [string[]]$Arguments
  )

  $output = & $SignTool @Arguments 2>&1
  $exitCode = $LASTEXITCODE
  $output | ForEach-Object { Write-Host $_ }
  if ($exitCode -ne 0) {
    throw "signtool failed with exit code $exitCode."
  }
}

$signTool = Find-SignTool
$files = @(Resolve-TargetFiles)
if ($files.Count -eq 0) {
  throw "No Windows build outputs were found. Run npm run tauri:build first."
}

foreach ($file in $files) {
  $common = @(
    "sign",
    "/fd", "SHA256",
    "/tr", $TimestampServer,
    "/td", "SHA256",
    "/d", "LocalStack Pro",
    "/du", "https://artnext.ru"
  )

  if ($PfxPath) {
    if (-not (Test-Path -LiteralPath $PfxPath)) {
      throw "PFX file was not found: $PfxPath"
    }
    $args = $common + @("/f", (Resolve-Path -LiteralPath $PfxPath).Path)
    if ($PfxPassword) {
      $args += @("/p", $PfxPassword)
    }
  } elseif ($Thumbprint) {
    $args = $common + @("/sha1", $Thumbprint.Replace(" ", ""))
  } else {
    $args = $common + @("/n", $CertificateSubject)
  }

  $args += @($file)
  Invoke-SignTool -SignTool $signTool -Arguments $args
  Write-Host "Signed with signtool: $file"
}

$releaseDir = "release"
New-Item -ItemType Directory -Force -Path $releaseDir | Out-Null
$installer = Join-Path "src-tauri\target\release\bundle\nsis" $installerFileName
if (Test-Path -LiteralPath $installer) {
  Copy-Item -LiteralPath $installer -Destination (Join-Path $releaseDir $installerFileName) -Force
  Write-Host "Release installer copied to release\$installerFileName"
}

foreach ($file in $files) {
  try {
    Invoke-SignTool -SignTool $signTool -Arguments @("verify", "/pa", "/v", $file)
  } catch {
    Write-Warning "Signature was added, but Windows chain verification did not pass for $file. Use a trusted OV/EV/Trusted Signing certificate for SmartScreen reputation."
  }
}
