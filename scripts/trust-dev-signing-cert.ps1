$ErrorActionPreference = "Stop"

$certPath = Join-Path (Resolve-Path ".").Path "certs\localstack-pro-dev-signing.cer"
if (-not (Test-Path -LiteralPath $certPath)) {
  throw "Development signing certificate file was not found: $certPath"
}

certutil.exe -user -addstore TrustedPublisher $certPath | Out-Host
if ($LASTEXITCODE -ne 0) {
  throw "Cannot add development certificate to CurrentUser\TrustedPublisher."
}

certutil.exe -user -addstore Root $certPath | Out-Host
if ($LASTEXITCODE -ne 0) {
  throw "Cannot add development certificate to CurrentUser\Root."
}

Write-Host "Trusted development signing certificate for the current Windows user."
