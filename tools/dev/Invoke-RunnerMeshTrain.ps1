#Requires -Version 7.0
[CmdletBinding()]
param(
    [string] $Model,
    [string] $Profile,
    [string] $Prompt,
    [string] $CodexCommand = 'codex',
    [Parameter(DontShow)]
    [string] $TestExecutionStateLogPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$EsContinuous = [Convert]::ToUInt32('80000000', 16)
$EsSystemRequired = [uint32]0x00000001

function Set-RunnerMeshExecutionState {
    param([uint32] $State)

    if ($TestExecutionStateLogPath) {
        [System.IO.File]::AppendAllText(
            $TestExecutionStateLogPath,
            ([string]$State + [Environment]::NewLine),
            [System.Text.UTF8Encoding]::new($false)
        )
        return
    }

    if (-not $IsWindows) {
        throw 'Invoke-RunnerMeshTrain.ps1 supports Windows only.'
    }

    if (-not ('RunnerMesh.Dev.NativePower' -as [type])) {
        Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;

namespace RunnerMesh.Dev
{
    public static class NativePower
    {
        [DllImport("kernel32.dll", SetLastError = true)]
        public static extern uint SetThreadExecutionState(uint executionState);
    }
}
'@
    }

    $result = [RunnerMesh.Dev.NativePower]::SetThreadExecutionState($State)
    if ($result -eq 0) {
        $errorCode = [Runtime.InteropServices.Marshal]::GetLastWin32Error()
        throw "SetThreadExecutionState failed with Win32 error $errorCode."
    }
}

$childArguments = [System.Collections.Generic.List[string]]::new()
if ($Profile) {
    $childArguments.Add('--profile')
    $childArguments.Add($Profile)
}
if ($Model) {
    $childArguments.Add('--model')
    $childArguments.Add($Model)
}
if ($Prompt) {
    # The option terminator makes the entire value a prompt. It cannot become a
    # Codex subcommand or a trust, permission, configuration, or path override.
    $childArguments.Add('--')
    $childArguments.Add($Prompt)
}

$inhibitionActive = $false
$childExitCode = 0
try {
    Set-RunnerMeshExecutionState -State ([uint32]($EsContinuous -bor $EsSystemRequired))
    $inhibitionActive = $true

    $nativeErrorPreferenceAvailable = Test-Path Variable:PSNativeCommandUseErrorActionPreference
    if ($nativeErrorPreferenceAvailable) {
        $previousNativeErrorPreference = $PSNativeCommandUseErrorActionPreference
        $PSNativeCommandUseErrorActionPreference = $false
    }
    try {
        & $CodexCommand @childArguments
        if ($null -ne $LASTEXITCODE) {
            $childExitCode = [int]$LASTEXITCODE
        }
        elseif (-not $?) {
            $childExitCode = 1
        }
    }
    finally {
        if ($nativeErrorPreferenceAvailable) {
            $PSNativeCommandUseErrorActionPreference = $previousNativeErrorPreference
        }
    }
}
finally {
    if ($inhibitionActive) {
        Set-RunnerMeshExecutionState -State $EsContinuous
    }
}

exit $childExitCode
