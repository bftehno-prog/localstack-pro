param(
  [switch]$Force,
  [switch]$KeepDownloads,
  [switch]$KeepExtracted
)

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$root = Resolve-Path (Join-Path $PSScriptRoot "..")
$bundle = Join-Path $root "src-tauri\bundled-services"
$downloads = Join-Path $bundle "_downloads"
$packagesDir = Join-Path $bundle "packages"
New-Item -ItemType Directory -Force -Path $bundle, $downloads | Out-Null

$packages = @(
  @{
    Id = "apache"
    Url = "https://www.apachelounge.com/download/VS18/binaries/httpd-2.4.67-260504-Win64-VS18.zip"
    Archive = "apache-httpd-2.4.67.zip"
    Target = "apache"
  },
  @{
    Id = "nginx"
    Url = "https://nginx.org/download/nginx-1.29.8.zip"
    Archive = "nginx-1.29.8.zip"
    Target = "nginx"
  },
  @{
    Id = "php"
    Url = "https://windows.php.net/downloads/releases/php-8.4.22-nts-Win32-vs17-x64.zip"
    Archive = "php-8.4.22-nts-Win32-vs17-x64.zip"
    Target = "php\php-8.4.22-nts-Win32-vs17-x64"
    DirectExtract = $true
  },
  @{
    Id = "mysql"
    Url = "https://dev.mysql.com/get/Downloads/MySQL-9.7/mysql-9.7.0-winx64.zip"
    Archive = "mysql-9.7.0-winx64.zip"
    Target = "mysql"
  },
  @{
    Id = "mariadb"
    Url = "https://archive.mariadb.org/mariadb-11.8.6/winx64-packages/mariadb-11.8.6-winx64.zip"
    Archive = "mariadb-11.8.6-winx64.zip"
    Target = "mariadb"
  },
  @{
    Id = "postgresql"
    Url = "https://get.enterprisedb.com/postgresql/postgresql-18.4-1-windows-x64-binaries.zip"
    Archive = "postgresql-18.4-binaries.zip"
    Target = "postgresql"
  },
  @{
    Id = "redis"
    Url = "https://github.com/tporadowski/redis/releases/download/v5.0.14.1/Redis-x64-5.0.14.1.zip"
    Archive = "Redis-x64-5.0.14.1.zip"
    Target = "redis\Redis-x64-5.0.14.1"
    DirectExtract = $true
  },
  @{
    Id = "mailpit"
    Url = "https://github.com/axllent/mailpit/releases/download/v1.30.0/mailpit-windows-amd64.zip"
    Archive = "mailpit-windows-amd64.zip"
    Target = "mailpit"
    DirectExtract = $true
  },
  @{
    Id = "nodejs"
    Url = "https://nodejs.org/dist/v26.2.0/node-v26.2.0-win-x64.zip"
    Archive = "node-v26.2.0-win-x64.zip"
    Target = "nodejs"
  }
)

New-Item -ItemType Directory -Force -Path $packagesDir | Out-Null
Remove-Item -LiteralPath (Join-Path $packagesDir "localstack-services.zip") -Force -ErrorAction SilentlyContinue

foreach ($package in $packages) {
  $archive = Join-Path $downloads $package.Archive
  $packageArchive = Join-Path $packagesDir $package.Archive
  $target = Join-Path $bundle $package.Target
  if ((Test-Path $packageArchive) -and -not $Force) {
    Write-Host "ready package $($package.Id)"
    continue
  }
  if ((Test-Path $target) -and -not $Force) {
    Write-Host "ready $($package.Id)"
  } else {
    if ((Test-Path $target) -and $Force) {
      Remove-Item -LiteralPath $target -Recurse -Force
    }
    New-Item -ItemType Directory -Force -Path $target | Out-Null
  }
  if (!(Test-Path $archive) -or $Force) {
    Write-Host "download $($package.Id)"
    $curl = Get-Command curl.exe -ErrorAction SilentlyContinue
    if ($curl) {
      & $curl.Source -L --fail --retry 3 --output $archive $package.Url
      if ($LASTEXITCODE -ne 0) {
        throw "curl failed for $($package.Id) with exit code $LASTEXITCODE"
      }
    } else {
      Invoke-WebRequest -UseBasicParsing -MaximumRedirection 10 -Uri $package.Url -OutFile $archive
    }
  }
  Copy-Item -LiteralPath $archive -Destination $packageArchive -Force
  if (!(Test-Path $target) -or $Force) {
    Write-Host "extract $($package.Id)"
    if ($package.DirectExtract) {
      Expand-Archive -LiteralPath $archive -DestinationPath $target -Force
    } else {
      Expand-Archive -LiteralPath $archive -DestinationPath (Join-Path $bundle $package.Id) -Force
    }
  }
}

$contentRoots = @("apache", "nginx", "php", "mysql", "mariadb", "postgresql", "redis", "mailpit", "nodejs") |
  ForEach-Object { Join-Path $bundle $_ } |
  Where-Object { Test-Path $_ }

if (!$KeepDownloads -and (Test-Path $downloads)) {
  Remove-Item -LiteralPath $downloads -Recurse -Force
}
if (!$KeepExtracted) {
  foreach ($path in $contentRoots) {
    if ($path.StartsWith($bundle)) {
      Remove-Item -LiteralPath $path -Recurse -Force
    }
  }
}

Write-Host "bundled services prepared: $bundle"
