# GTA VI con VIctory, build de diagnostico: DRED (nombra el comando de GPU que cuelga),
# capa de debug de D3D12 y log detallado a archivo. El juego va mas lento.
$ErrorActionPreference = "Stop"
$env:Path += ";$env:USERPROFILE\.cargo\bin"
$env:CARGO_TARGET_DIR = "D:\victory-data\target"
$repo = "C:\Users\Mauricio\Desktop\LEONIDA\VIctory"
$dg   = "D:\victory-data\target\diag"
Set-Location $repo
cargo build --profile diag -p victory --features diagnostics
New-Item -ItemType Directory -Force "$dg\D3D12" | Out-Null
Copy-Item "D:\victory-data\agility\x64\D3D12Core.dll" "$dg\D3D12\" -Force
Copy-Item "D:\victory-data\agility\pkg\build\native\bin\x64\d3d12SDKLayers.dll" "$dg\D3D12\" -Force
$env:VICTORY_D3D12_DRED = "1"
# Capa de debug apagada (frena mucho). Para activarla: $env:VICTORY_D3D12_DEBUG = "1"
# Corta los loops de shaders traducidos a 64 vueltas por lane: si los frames de la
# cinematica se aceleran mucho, los loops largos son la causa de los ~1 fps.
$env:VICTORY_SHADER_LOOP_LIMIT = "64"
$log = "D:\victory-data\ultima_corrida.log"
# cmd hace el 2>&1 (PowerShell 5.1 convierte el stderr de un exe en errores).
$ErrorActionPreference = "Continue"
cmd /c "`"$dg\victory.exe`" run F:\GTAVI\GTA6-mllorente37dump\game_scarlett_master.exe --execute 2>&1" | Tee-Object -FilePath $log
