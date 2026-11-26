# Starts the title with the settings this fork is played with (see README.md).
#   scripts\play.ps1 -Exe <path of your own copy's executable> [-SeriesX]
# Series S mode (default): the title renders at 1280x720 and its interface is composed over it.
param(
    [Parameter(Mandatory)][string]$Exe,
    [switch]$SeriesX,
    [string]$Build = 'diag',
    # Windows 10: a folder with the D3D12 Agility SDK runtime (D3D12Core.dll), for shader model 6.6
    [string]$Agility = ''
)
$settings = [ordered]@{
    VICTORY_RT_STUB = '1'; VICTORY_POPS_SKIP = '1'; VICTORY_INTERP_ZERO = '1'
    VICTORY_PIPELINE_WAIT_ALL = '1'; VICTORY_TEXTURE_STAGE_MB = '512'; VICTORY_TEXTURE_STAGE_MS = '500'
    VICTORY_ASYNC_SUBMIT = '1'; VICTORY_QUEUE2 = '1'; VICTORY_COPY_PLAIN = '1'; VICTORY_TEXTURE_NO_WAIT = '1'
    VICTORY_INDIRECT_WINDOW = '1'; VICTORY_INDIRECT_GATHER = '1'; VICTORY_JOIN_PARTS = '1'; VICTORY_BACK_OWN_VIEW = '1'
    VICTORY_MERGE_RUNS = '65536'; VICTORY_WIDE_REFRESH = '1'; VICTORY_NO_WATCHDOG = '1'; VICTORY_RELAY = '2'
}
if ($Agility) { $settings.VICTORY_D3D12_AGILITY = $Agility }
$victory = Join-Path $PSScriptRoot "..\target\$Build\victory.exe"
if (-not (Test-Path $victory)) { Write-Host "No $victory. Build it first (README.md)."; return }
Get-ChildItem Env:VICTORY_* | ForEach-Object { Remove-Item "Env:$($_.Name)" }
foreach ($k in $settings.Keys) { Set-Item "Env:$k" $settings[$k] }
$console = if ($SeriesX) { @() } else { @('--console', 'series-s') }
Push-Location (Join-Path $PSScriptRoot '..')   # victory.toml is looked for here
try { & $victory run (Resolve-Path $Exe).Path --execute @console } finally { Pop-Location }
