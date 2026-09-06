[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('ColdIdle', 'SixAgents')]
    [string]$Scenario,
    [ValidateRange(0, 300)]
    [int]$SettleSeconds = 30
)

$ErrorActionPreference = 'Stop'
$releasePath = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '../src-tauri/target/release/herdr-companion.exe')).Path

function Get-ReleaseProcesses {
    @(Get-CimInstance Win32_Process -Property ProcessId, ParentProcessId, Name, ExecutablePath, CreationDate, WorkingSetSize, PrivatePageCount)
}

$initialProcesses = Get-ReleaseProcesses
$roots = @($initialProcesses | Where-Object { $_.ExecutablePath -eq $releasePath })
if ($roots.Count -ne 1) {
    throw "Expected exactly one running release at $releasePath; found $($roots.Count). Start that executable first."
}
$root = $roots[0]
if ($SettleSeconds -gt 0) { Start-Sleep -Seconds $SettleSeconds }

$processes = Get-ReleaseProcesses
$currentRoot = @($processes | Where-Object {
    $_.ProcessId -eq $root.ProcessId -and $_.CreationDate -eq $root.CreationDate -and $_.ExecutablePath -eq $releasePath
})
if ($currentRoot.Count -ne 1) { throw 'Release exited or restarted during measurement; repeat the sample.' }

# Restrict accounting to descendants of this exact release, never all WebView2 processes.
$group = @($currentRoot[0])
$seen = @{}
$seen[[uint32]$root.ProcessId] = $true
for ($index = 0; $index -lt $group.Count; $index++) {
    $parent = $group[$index]
    foreach ($child in $processes) {
        if ($child.ParentProcessId -eq $parent.ProcessId -and
            $child.CreationDate -ge $parent.CreationDate -and
            -not $seen.ContainsKey([uint32]$child.ProcessId)) {
            $group += $child
            $seen[[uint32]$child.ProcessId] = $true
        }
    }
}
$webviews = @($group | Where-Object { $_.Name -eq 'msedgewebview2.exe' })
if ($webviews.Count -eq 0) { throw 'No descendant WebView2 processes found; sample would be incomplete.' }

function Get-MemoryTotal($Members) {
    [pscustomobject]@{
        ProcessCount = @($Members).Count
        WorkingSetMiB = [math]::Round((($Members | Measure-Object -Property WorkingSetSize -Sum).Sum / 1MB), 2)
        PrivateMemoryMiB = [math]::Round((($Members | Measure-Object -Property PrivatePageCount -Sum).Sum / 1MB), 2)
    }
}

[pscustomobject]@{
    MeasuredAt = (Get-Date).ToUniversalTime().ToString('o')
    Scenario = $Scenario
    ScenarioSource = 'Operator selected; agent count and connection state must be confirmed in the UI'
    SettleSeconds = $SettleSeconds
    ReleasePath = $releasePath
    ReleaseSha256 = (Get-FileHash -LiteralPath $releasePath -Algorithm SHA256).Hash
    Companion = Get-MemoryTotal $currentRoot
    WebView2 = Get-MemoryTotal $webviews
    ApplicationGroup = Get-MemoryTotal $group
} | ConvertTo-Json -Depth 4
