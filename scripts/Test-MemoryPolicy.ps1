[CmdletBinding()]
param(
    [ValidateSet('Both', 'BackgroundOnly')]
    [string]$FocusMode = 'Both',
    [ValidateSet('Both', 'normal', 'adaptive')]
    [string]$Policy = 'Both'
)

$ErrorActionPreference = 'Stop'
if (@(Get-Process -Name herdr-companion -ErrorAction SilentlyContinue).Count -gt 0) {
    throw 'Close the existing Companion window before running the experiment.'
}
if (@(Get-Process -Name herdr -ErrorAction SilentlyContinue).Count -gt 0) {
    throw 'This cold-idle comparison requires Herdr to be absent. Do not stop active work to run it.'
}

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class CompanionMemoryFocus {
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
    [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr window);
}
'@

$originalForeground = [CompanionMemoryFocus]::GetForegroundWindow()
if ($originalForeground -eq [IntPtr]::Zero) { throw 'No foreground window available to restore focus to.' }
$releasePath = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '../src-tauri/target/release/herdr-companion.exe')).Path
$outputDirectory = Join-Path $PSScriptRoot ('../target/memory-experiment/' + (Get-Date -Format 'yyyyMMdd-HHmmss'))
$null = New-Item -ItemType Directory -Path $outputDirectory -Force
$outputDirectory = (Resolve-Path -LiteralPath $outputDirectory).Path
$savedPolicy = $env:HERDR_COMPANION_MEMORY_POLICY
$savedDiagnostics = $env:HERDR_COMPANION_MEMORY_DIAGNOSTICS
$samples = @()

try {
    $policies = if ($Policy -eq 'Both') { @('normal', 'adaptive') } else { @($Policy) }
    foreach ($selectedPolicy in $policies) {
        $env:HERDR_COMPANION_MEMORY_POLICY = $selectedPolicy
        $env:HERDR_COMPANION_MEMORY_DIAGNOSTICS = '1'
        $tracePath = Join-Path $outputDirectory ($selectedPolicy + '.stderr.log')
        $started = Start-Process -FilePath $releasePath -PassThru -RedirectStandardError $tracePath -RedirectStandardOutput (Join-Path $outputDirectory ($selectedPolicy + '.stdout.log'))
        try {
            $ready = [Diagnostics.Stopwatch]::StartNew()
            do {
                Start-Sleep -Milliseconds 200
                $started.Refresh()
                if ($started.HasExited) { throw 'Experiment Release exited before readiness.' }
            } while ($started.MainWindowHandle -eq [IntPtr]::Zero -and $ready.Elapsed.TotalSeconds -lt 15)
            $releaseWindow = $started.MainWindowHandle
            if ($releaseWindow -eq [IntPtr]::Zero) { throw 'Release window was not created.' }
            # The native window appears before WebView2 has finished its focus setup.
            Start-Sleep -Seconds 5

            $focusStates = if ($FocusMode -eq 'BackgroundOnly') { @('Unfocused') } else { @('Focused', 'Unfocused') }
            foreach ($focus in $focusStates) {
                $desiredWindow = if ($focus -eq 'Focused') { $releaseWindow } else { $originalForeground }
                if (-not [CompanionMemoryFocus]::IsWindow($desiredWindow)) { throw 'Focus destination no longer exists.' }
                $null = [CompanionMemoryFocus]::SetForegroundWindow($desiredWindow)
                Start-Sleep -Milliseconds 500
                $isFocused = [CompanionMemoryFocus]::GetForegroundWindow() -eq $releaseWindow
                if ($isFocused -ne ($focus -eq 'Focused')) {
                    throw "Windows did not grant $focus focus; sample would be invalid."
                }
                Write-Output "Sampling policy=$selectedPolicy focus=$focus after 30 seconds..."
                # Observe throughout the settling interval; discard samples interrupted by focus changes.
                $stableSeconds = 0
                for ($second = 0; $second -lt 120 -and $stableSeconds -lt 30; $second++) {
                    Start-Sleep -Seconds 1
                    $isFocused = [CompanionMemoryFocus]::GetForegroundWindow() -eq $releaseWindow
                    if ($isFocused -ne ($focus -eq 'Focused')) {
                        $stableSeconds = 0
                    } else {
                        $stableSeconds++
                    }
                }
                if ($stableSeconds -lt 30) { throw 'No uninterrupted 30-second focus interval; sample discarded.' }
                if (@(Get-Process -Name herdr -ErrorAction SilentlyContinue).Count -gt 0) {
                    throw 'Herdr started during the cold-idle comparison; sample would mix scenarios.'
                }
                $sample = & (Join-Path $PSScriptRoot 'Measure-ReleaseMemory.ps1') -Scenario ColdIdle -SettleSeconds 0 | ConvertFrom-Json
                $lastPolicy = @(Get-Content -LiteralPath $tracePath | Where-Object { $_ -like 'memory_policy *' }) | Select-Object -Last 1
                $expectedLevel = if ($selectedPolicy -eq 'adaptive' -and $focus -eq 'Unfocused') { 1 } else { 0 }
                $expectedFocus = if ($focus -eq 'Focused') { 'true' } else { 'false' }
                if ($lastPolicy -ne "memory_policy focused=$expectedFocus requested=$expectedLevel actual=$expectedLevel") {
                    throw 'Policy was not verified. Build with --features webview-memory-experiment and check the diagnostic log.'
                }
                $sample.SettleSeconds = 30
                $sample | Add-Member -NotePropertyName Policy -NotePropertyValue $selectedPolicy
                $sample | Add-Member -NotePropertyName Focus -NotePropertyValue $focus
                $sample | Add-Member -NotePropertyName PolicyReadback -NotePropertyValue $lastPolicy
                $samples += $sample
                $samples | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $outputDirectory 'samples.json') -Encoding UTF8
                Write-Output "PrivateMemoryMiB=$($sample.ApplicationGroup.PrivateMemoryMiB) WorkingSetMiB=$($sample.ApplicationGroup.WorkingSetMiB)"
            }
        } finally {
            # Only ask the exact process created by this script to close; never terminate user processes.
            try {
                $started.Refresh()
                if (-not $started.HasExited) {
                    $null = $started.CloseMainWindow()
                    if (-not $started.WaitForExit(5000)) { throw 'Experiment window did not close; close it manually.' }
                }
            } finally {
                $null = [CompanionMemoryFocus]::SetForegroundWindow($originalForeground)
            }
        }
    }
} finally {
    $env:HERDR_COMPANION_MEMORY_POLICY = $savedPolicy
    $env:HERDR_COMPANION_MEMORY_DIAGNOSTICS = $savedDiagnostics
}
Write-Output "Evidence directory: $outputDirectory"
