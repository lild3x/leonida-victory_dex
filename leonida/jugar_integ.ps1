# GTA VI con la rama de integracion (main + 12 PRs de shaders/GPU/NVIDIA + Agility, ver abajo).
# Worktree: D:\victory-data\wt_integ  (rama "integ")  -  exe: D:\victory-data\target_integ\release
# PRs integrados: 969 (Agility) 1425 298 1257 1312 1057 1073 1083 1467 1523 1129 1407
# Log completo a D:\victory-data\ultima_corrida_integ.log
$env:Path += ";$env:USERPROFILE\.cargo\bin"
$env:CARGO_TARGET_DIR = "D:\victory-data\target_integ"
$env:VICTORY_D3D12_AGILITY = "D:\victory-data\agility\x64"
Set-Location "D:\victory-data\wt_integ"
cargo build --release -p victory
New-Item -ItemType Directory -Force "$env:CARGO_TARGET_DIR\release\D3D12" | Out-Null
Copy-Item "D:\victory-data\agility\x64\D3D12Core.dll" "$env:CARGO_TARGET_DIR\release\D3D12\" -Force
$ErrorActionPreference = "Continue"
cmd /c "`"$env:CARGO_TARGET_DIR\release\victory.exe`" run F:\GTAVI\GTA6-mllorente37dump\game_scarlett_master.exe --execute 2>&1" | Tee-Object -FilePath "D:\victory-data\ultima_corrida_integ.log"
