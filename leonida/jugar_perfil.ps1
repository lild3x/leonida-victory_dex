# GTA VI con el fork de fxceless + nuestros atajos. Para JUGAR (mover el personaje).
#   - Enter automatico solo en los primeros ~100 s (menus de arranque); despues manejas vos.
#   - Hace falta hacer CLICK en la ventana del juego para que reciba el teclado.
#   - Sin gamepad, el teclado hace de control:
#       W A S D = stick izquierdo (mover)    I J K L = stick derecho (camara)
#       flechas = cruceta   Enter = A   Backspace = B   X = X   Y = Y
#       Q/E = gatillos-hombros   Z/C = gatillos   F/H = click de sticks   Esc = Menu   Tab = Vista
# Si hay un gamepad XInput conectado, se usa ese.
$env:Path += ";$env:USERPROFILE\.cargo\bin"
Get-Process victory -ErrorAction SilentlyContinue | Stop-Process -Force
Get-ChildItem Env: | Where-Object { $_.Name -like "VICTORY_*" } | ForEach-Object { Remove-Item "Env:$($_.Name)" -ErrorAction SilentlyContinue }
$env:VICTORY_D3D12_AGILITY = "F:\victory-data\agility\x64"   # Windows 10: SM 6.6
$env:VICTORY_RT_STUB = "1"        # atajo: sin sombras de ray tracing, pero zonas iluminadas
$env:VICTORY_POPS_SKIP = "1"      # atajo: caras/personajes (ordenamiento de pixeles POPS)
$env:VICTORY_INTERP_ZERO = "1"    # atajo: shaders deferred de pisos/paredes (PS_DeferredTextured_*) traducen
$env:VICTORY_GPU_TIMING = "1"   # PERFIL: tiempo de GPU por comando
$env:VICTORY_RELAY = "2"          # velocidad
$env:VICTORY_KEYS = (30..100 | Where-Object { $_ % 9 -eq 0 } | ForEach-Object { "${_}:Enter" }) -join ","
Copy-Item D:\victory-data\target_fx\diag\victory.exe F:\victory-data\run\victory.exe -Force -ErrorAction SilentlyContinue
Set-Location F:\victory-data\run
$log = "F:\victory-data\ultima_corrida_perfil.log"
cmd /c "`"F:\victory-data\run\victory.exe`" run F:\GTAVI\GTA6-mllorente37dump\game_scarlett_master.exe --execute 2>&1" | Tee-Object -FilePath $log


