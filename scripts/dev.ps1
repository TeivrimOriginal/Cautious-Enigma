$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

if (-not (Test-Path .env)) {
    Copy-Item .env.example .env
    Write-Host "Создан .env из .env.example"
}

if (-not (Get-Command docker -ErrorAction SilentlyContinue)) {
    throw "Docker не найден. Запусти PostgreSQL отдельно и выполни cargo run."
}

docker compose up --build
