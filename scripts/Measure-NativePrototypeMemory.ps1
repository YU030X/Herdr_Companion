[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('ColdIdle', 'SixAgents')]
    [string]$Scenario,
    [ValidateRange(0, 300)]
    [int]$SettleSeconds = 30
)

$prototypePath = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '../native-prototype/target/release/herdr-companion-native-prototype.exe') -ErrorAction Stop).Path
& (Join-Path $PSScriptRoot 'Measure-ReleaseMemory.ps1') -Scenario $Scenario -SettleSeconds $SettleSeconds -ExecutablePath $prototypePath
