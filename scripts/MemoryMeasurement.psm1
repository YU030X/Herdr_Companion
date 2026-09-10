function Get-MemoryTargetPath {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [string]$ScriptRoot,
        [string]$ExecutablePath
    )

    if ($ExecutablePath) {
        return $ExecutablePath
    }

    Join-Path $ScriptRoot '../src-tauri/target/release/herdr-companion.exe'
}

function Resolve-MemoryTarget {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [string]$ScriptRoot,
        [string]$ExecutablePath
    )

    $targetPath = Get-MemoryTargetPath -ScriptRoot $ScriptRoot -ExecutablePath $ExecutablePath
    $releasePath = (Resolve-Path -LiteralPath $targetPath -ErrorAction Stop).Path
    $nativePath = Join-Path $ScriptRoot '../native-prototype/target/release/herdr-companion-native-prototype.exe'
    $canonicalNativePath = if (Test-Path -LiteralPath $nativePath -PathType Leaf) {
        (Resolve-Path -LiteralPath $nativePath -ErrorAction Stop).Path
    }

    [pscustomobject]@{
        Path = $releasePath
        Name = [IO.Path]::GetFileName($releasePath)
        IsNativePrototype = [bool]($canonicalNativePath -and $releasePath -ieq $canonicalNativePath)
    }
}

Export-ModuleMember -Function Get-MemoryTargetPath, Resolve-MemoryTarget
