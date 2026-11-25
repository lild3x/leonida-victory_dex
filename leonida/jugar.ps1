# Lanza GTA VI con VIctory. Corre desde la carpeta del repo para que lea victory.toml
# (cache y datos en D:) y asegura que D3D12Core.dll (Agility SDK) este junto al exe.
$ErrorActionPreference = "Stop"
$env:Path += ";$env:USERPROFILE\.cargo\bin"
$env:CARGO_TARGET_DIR = "D:\victory-data\target"
$repo = "C:\Users\Mauricio\Desktop\LEONIDA\VIctory"
$rel  = "D:\victory-data\target\release"
Set-Location $repo
cargo build --release -p victory
New-Item -ItemType Directory -Force "$rel\D3D12" | Out-Null
Copy-Item "D:\victory-data\agility\x64\D3D12Core.dll" "$rel\D3D12\" -Force
& "$rel\victory.exe" run "F:\GTAVI\GTA6-mllorente37dump\game_scarlett_master.exe" --execute
