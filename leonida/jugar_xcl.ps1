# GTA VI con xcl (xcl-dev/xcl), otra capa de compatibilidad, para comparar con VIctory.
# Todo lo suyo va a F: (C: se llena). No toca nada de VIctory.
Get-Process xcl -ErrorAction SilentlyContinue | Stop-Process -Force
$env:XCL_DATA_ROOT  = "F:\xcl-data"                     # xcl.toml, saves, cache, logs\xcl.log
$env:XCL_DXIL_CACHE = "F:\xcl-data\dxil"                # cache de shaders traducidos
$env:XCL_D3D12_DIR  = "F:\victory-data\agility\x64"     # Agility SDK (Windows 10: SM 6.6)
$env:XCL_DXC_DIR    = "D:\victory-data\dxc\bin\x64"     # DXC v1.9.2609 oficial
$env:XCL_HUGE_PSO_CONCURRENCY = "2"   # 2 compilaciones enormes a la vez (default 1)
$env:XCL_DIRECT_DXIL = "1"            # DXIL directo, sin pasar por HLSL + DXC (experimental)
New-Item -ItemType Directory -Force $env:XCL_DATA_ROOT, $env:XCL_DXIL_CACHE | Out-Null
Set-Location D:\victory-data\xcl
& "D:\victory-data\target_xcl\release\xcl.exe" run "F:\GTAVI\GTA6-mllorente37dump" 2>&1 | Tee-Object -FilePath "F:\xcl-data\ultima_corrida_xcl.log"

