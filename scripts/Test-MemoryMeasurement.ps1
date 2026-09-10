[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'MemoryMeasurement.psm1') -Force

$temporaryRoot = Join-Path ([IO.Path]::GetTempPath()) ('herdr-memory-test-' + [Guid]::NewGuid().ToString('N'))
$scriptsRoot = Join-Path $temporaryRoot 'scripts'
$productionPath = Join-Path $temporaryRoot 'src-tauri/target/release/herdr-companion.exe'
$nativePath = Join-Path $temporaryRoot 'native-prototype/target/release/herdr-companion-native-prototype.exe'
$null = New-Item -ItemType Directory -Path (Split-Path -Parent $productionPath) -Force
$null = New-Item -ItemType Directory -Path (Split-Path -Parent $nativePath) -Force
$null = New-Item -ItemType Directory -Path $scriptsRoot -Force
$null = New-Item -ItemType File -Path $productionPath
$null = New-Item -ItemType File -Path $nativePath

try {
    $defaultTarget = Resolve-MemoryTarget -ScriptRoot $scriptsRoot
    if ($defaultTarget.Path -ne (Resolve-Path -LiteralPath $productionPath).Path) {
        throw 'Default target did not resolve to the production executable.'
    }
    if ($defaultTarget.IsNativePrototype) {
        throw 'Default production target was classified as the native prototype.'
    }

    $nativeTarget = Resolve-MemoryTarget -ScriptRoot $scriptsRoot -ExecutablePath $nativePath
    if (-not $nativeTarget.IsNativePrototype) {
        throw 'Canonical native target was not classified as the native prototype.'
    }

    $missingPath = Join-Path $temporaryRoot 'missing.exe'
    $missingFailed = $false
    try {
        Resolve-MemoryTarget -ScriptRoot $scriptsRoot -ExecutablePath $missingPath | Out-Null
    } catch {
        $missingFailed = $true
    }
    if (-not $missingFailed) {
        throw 'Missing target unexpectedly resolved successfully.'
    }

    Write-Output 'Memory target classification tests passed.'
} finally {
    Remove-Item -LiteralPath $temporaryRoot -Recurse -Force
}
