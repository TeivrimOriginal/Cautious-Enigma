$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$site = Join-Path $PSScriptRoot "site"

if (Test-Path $site) {
    Remove-Item $site -Recurse -Force
}
New-Item -ItemType Directory -Path $site -Force | Out-Null
New-Item -ItemType Directory -Path (Join-Path $site "data") -Force | Out-Null

Copy-Item (Join-Path $root "web\*") $site -Force
Copy-Item (Join-Path $root "data\*") (Join-Path $site "data") -Force
Copy-Item (Join-Path $root "static\style.css") (Join-Path $site "style.css") -Force
Write-Host "Desktop site synced: $site" -ForegroundColor Green
