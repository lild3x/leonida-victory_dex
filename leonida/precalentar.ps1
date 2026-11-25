# Precalienta SIN jugar: traduce los shaders de shaders.rpf al cache y arma los pipelines de compute (victory warm, de victory-amd).
# Usar los MISMOS atajos que jugar_fx.ps1: si cambia uno, o cambia el traductor
# (crates/victory-shader o shader_cache.rs), la clave cambia y hay que repetir.
# Los pipelines de dibujo (no compute) siguen compilandose en la primera corrida.
Copy-Item D:\victory-data\target_fx\diag\victory.exe F:\victory-data\run\victory.exe -Force -ErrorAction SilentlyContinue
Set-Location F:\victory-data\run
$env:VICTORY_D3D12_AGILITY = "F:\victory-data\agility\x64"   # el warm arma pipelines: necesita el Agility
$env:VICTORY_HOST_MAX_WAVE = "32"   # waves de 32 lanes (RTX 5060 Ti), como el runtime
$env:VICTORY_RT_STUB = "1"
$env:VICTORY_POPS_SKIP = "1"
$env:VICTORY_INTERP_ZERO = "1"
& "F:\victory-data\run\victory.exe" warm F:\GTAVI\GTA6-mllorente37dump\shaders.rpf 2>&1 | Tee-Object -FilePath F:\victory-data\warm.log
