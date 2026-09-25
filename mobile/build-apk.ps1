param(
    [switch]$Offline
)

$ErrorActionPreference = 'Stop'
$mobile = $PSScriptRoot

if (-not $env:JAVA_HOME -and (Test-Path 'D:\12344\jbr')) {
    $env:JAVA_HOME = 'D:\12344\jbr'
}
if (-not $env:ANDROID_HOME -and (Test-Path 'C:\Users\teivrim\AppData\Local\Android\Sdk')) {
    $env:ANDROID_HOME = 'C:\Users\teivrim\AppData\Local\Android\Sdk'
}

$gradleArgs = @(':app:assembleDebug', '--no-daemon')
if ($Offline) { $gradleArgs += '--offline' }

Push-Location $mobile
try {
    & (Join-Path $mobile 'gradlew.bat') @gradleArgs
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    Write-Host "APK: $mobile\app\build\outputs\apk\debug\app-debug.apk" -ForegroundColor Green
}
finally {
    Pop-Location
}
