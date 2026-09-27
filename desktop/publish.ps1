#Requires -Version 5
<#
.SYNOPSIS
    Публикация десктопного приложения LinguaRust на crates.io.

.DESCRIPTION
    Скрипт проверяет пакет и загружает его в реестр. Перед первым запуском
    нужна авторизация: cargo login (токен берётся на https://crates.io/me/settings/tokens).

    Перед публикацией пакет версионируется, поэтому каждую следующую версию
    нужно поднять в desktop/Cargo.toml.
#>
[CmdletBinding()]
param(
    # Только собрать и проверить пакет, ничего не отправляя.
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Push-Location $root

try {
    Write-Host "==> Проверки кода" -ForegroundColor Cyan
    cargo fmt --manifest-path desktop/Cargo.toml -- --check
    cargo clippy --manifest-path desktop/Cargo.toml --all-targets -- -D warnings
    cargo test --manifest-path desktop/Cargo.toml

    Write-Host "==> Синхронизация встроенного сайта" -ForegroundColor Cyan
    & "$PSScriptRoot\sync-site.ps1"

    Write-Host "==> Сборка пакета" -ForegroundColor Cyan
    cargo package --manifest-path desktop/Cargo.toml

    $crate = Get-ChildItem desktop\target\package -Filter *.crate |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1
    if (-not $crate) {
        throw "архив пакета не найден в desktop\target\package"
    }
    Write-Host "Готов пакет: $($crate.Name) ($([math]::Round($crate.Length / 1KB, 1)) КБ)" -ForegroundColor Green

    if ($DryRun) {
        Write-Host "Режим -DryRun: отправка пропущена" -ForegroundColor Yellow
        return
    }

    # Зеркало crates.io в .cargo/config.toml мешает публикации, поэтому
    # публикуем с исходным реестром и только на время этой команды.
    $config = Join-Path $root ".cargo\config.toml"
    $mirrorArgs = @()
    if (Test-Path $config) {
        Write-Host "Временно убираем зеркало реестра ($config)" -ForegroundColor Yellow
        Rename-Item $config "config.toml.disabled"
        $mirrorArgs = @("--config", "registries.crates-io.protocol=sparse")
    }

    try {
        Write-Host "==> Публикация на crates.io" -ForegroundColor Cyan
        cargo publish --manifest-path desktop/Cargo.toml
    }
    finally {
        if (Test-Path "$config.disabled") {
            Rename-Item "$config.disabled" "config.toml"
            Write-Host "Зеркало реестра возвращено" -ForegroundColor Yellow
        }
    }
}
finally {
    Pop-Location
}
