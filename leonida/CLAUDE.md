# LEONIDA — GTA VI sobre VIctory (instrucciones del proyecto)

Objetivo: correr GTA VI (dump propio de Xbox Series) en esta PC con **VIctory** (capa de compatibilidad GPL de
Marvisak) hasta ver el mundo 3D bien iluminado. Prioridad: que se vea bien; los fps se optimizan después.

## Cómo hablarme
- Español rioplatense. Informes cortos: qué se midió, qué falló, qué sigue. No pegar logs largos: resumir.

## Reglas duras
- **Nada del juego entra al repo ni a PRs/issues:** ni shaders, ni desensamblado, ni capturas, ni dumps
  (docs/asset-policy.md). Todo eso queda en `D:\victory-data`. En PRs se describe con palabras, hashes y números.
- **Publicar requiere mi OK:** fork, push, PR, comentarios y cierres a nombre mío. Antes de abrir un PR, buscar
  duplicados (hay 150+ PRs abiertos que se pisan). Los PRs/comentarios llevan la línea de IA-asistido.
- Software de terceros (RenderDoc, forks, paquetes): revisar el código antes de compilar/instalar y avisar.
- Cada corrida del juego puede colgar la GPU y apagar los monitores un momento: avisar. No tocar el teclado
  en la ventana del juego mientras corre una prueba.

## Entorno
- Windows 10 IoT LTSC 21H2 (19044), RTX 5060 Ti 16 GB (waves de **32** lanes), 32 GB RAM. C: casi lleno: usar D:.
  Archivo de paginacion movido a F: (40-60 GB fijo; C: 2-4 GB): el juego compromete ~20 GB y antes llenaba C:.
- Dump del juego: `F:\GTAVI\GTA6-mllorente37dump\game_scarlett_master.exe`.
- Rust 1.98.1 (`$env:Path += ";$env:USERPROFILE\.cargo\bin"`). Builds a D: con `CARGO_TARGET_DIR=D:\victory-data\target*`.
  El perfil debug no enlaza (LNK1140): usar `--release`, o `--profile diag -p victory --features diagnostics`.
- `victory.toml` (ignorado por git, en la carpeta del repo/worktree desde donde se corre): `game_dir`,
  `cache_dir=F:\victory-data\cache`, `data_dir=F:\victory-data\data`. Correr SIEMPRE desde la carpeta del repo.
  **D: es un HDD mecanico** (lecturas aleatorias ~5 MB/s): cache y datos del juego van en F: (SSD SATA).
  En D: quedan solo builds (`target_*`), worktrees y volcados. **Al correr, TODO va por F:**: exe en
  `F:\victory-data\run` (los scripts lo copian desde `target_fx`), Agility en `F:\victory-data\agility`, logs en `F:\victory-data\ultima_corrida*.log`. La clave de la cache cambia al recompilar o al
  cambiar un atajo: evitar recompilar sin necesidad.
- **Windows 10 necesita el Agility SDK** (SM 6.6): `D:\victory-data\agility\x64\D3D12Core.dll`, con
  `VICTORY_D3D12_AGILITY` apuntando ahí (PR upstream #969). Sin eso: pantalla negra (`root signature flags c01`).
- **No usar RenderDoc:** capturar un frame del mundo (~24 GB de memoria del título) cuelga el juego y llena
  C: con el archivo de paginación. Se sacó (movido a `D:\_PARA_BORRAR_2026-11-24\RenderDoc_sacado`). Para ver
  la GPU: volcado de targets del propio fork (`VICTORY_DUMP_TARGETS`), analizado con números.

## Árboles de código
- `LEONIDA\VIctory` — clon de Marvisak/VIctory (ramas locales: `main`, `agility-sdk`, `shader-trace-missing-origin`).
  Fork propio mío: `mauri9911/VIctory`. Mi PR #1518 se cerró como duplicado de #969.
- `D:\victory-data\wt_integ` (rama `integ`) — main + 12 PRs de shaders/GPU/NVIDIA. Mejor que main, no llega a 3D.
- **`D:\victory-data\fx_gta6`, rama `fx-amd` — EL PRINCIPAL (desde 24/11):** `fx-local` + merge de
  `myokex/victory-amd` (remote `amd`): texturas por DirectStorage que la GPU no recopiaba, mips chicos, compute,
  `victory warm` (traduce + arma pipelines de compute). Mejor imagen (ropa, paredes, piso) y ~2,9 fps / 340 ms
  estables. `fx-local` (fork `fxceless/victory-fork` + #969 + nuestros atajos) queda como respaldo; commits locales, sin push.
- Revisados y descartados: `Zettrox/ztrx` = mismo codigo que fxceless `2119b0e` (solo otro README); `xcl-dev/xcl`
  comparte memoria sin copias pero exige reescribir la reserva de memoria (probado solo Win11/AMD).
- **xcl probado el 24/11** (`D:\victory-data\xcl`, build en `target_xcl`, lanzador `jugar_xcl.ps1`, datos en `F:\xcl-data`,
  DXC 1.9.2609 en `D:\victory-data\dxc`): arranca y dibuja menus, pero precompila ~58.700 pipelines de a 2-4 hilos
  (~32/min = ~30 h) y crashea (0xc0000005 en VCRUNTIME140) al pasar al 3D con `XCL_DIRECT_DXIL=1` +
  `XCL_HUGE_PSO_CONCURRENCY=2`. Descartado por ahora; seguimos con VIctory.
- Medido y descartado en esta PC: `VICTORY_INPLACE=1` (solo 4 MB sin copia, 1,8 fps), `VICTORY_NO_BACK_*`
  (baja "vuelta" pero no el frame), combo `LATE_WAIT=3 WIDE_BACK=1 CARD_BUFFERS_*` (630 ms, picos de 2,9 s).
  El cuello del frame es "preparar". Seguir con `RELAY=2`.

## Receta de prueba (build diag)
`VICTORY_KEYS="30:Enter,39:Enter,..."` (Enter cada 9 s) para pasar menús; `VICTORY_CAPTURE=<dir>`,
`VICTORY_CAPTURE_WORLD=1`, `VICTORY_CAPTURE_WORLD_ALSO=118,160,...` para capturas del mundo; `VICTORY_RELAY=2`
(velocidad); `VICTORY_D3D12_DRED=1` y `VICTORY_GPU_TIMING=1` para cuelgues; `VICTORY_SKIP_PIPELINES=<hashes>`.
Logs de crash: `F:\victory-data\data\7952987A\crashes.log`. Barrido offline sin GPU: `victory translate <shaders.rpf> --list`.
**Precalentado:** `precalentar.ps1` (`victory warm` sobre shaders.rpf, ~8 min). Repetirlo despues de cada cambio del
traductor (`crates/victory-shader`) o de un atajo que entra en la clave; si no, el juego arranca en frio y tarda.

## Atajos locales en fx-local (todos apagados por defecto, marcados `shortcut:` en el codigo)
`VICTORY_RT_STUB` (rayos de sombra sin geometria: zonas iluminadas sin sombras), `VICTORY_POPS_SKIP` (caras:
shaders con POPS sin orden de olas), `VICTORY_INTERP_ZERO` (pisos/paredes: PS_DeferredTextured_* traducen con
la 1ra mitad de interpolacion en cero). Los tres van en `jugar_fx.ps1`. Entran en la clave de la cache de shaders.
Llegamos a jugabilidad real (HUD, texto en espanol) a ~2 fps. Falta: luz indirecta, lamparas, rayas en el cielo.
El volcado de targets se pide relativo al mundo: `VICTORY_CAPTURE_WORLD=1` + `VICTORY_CAPTURE_WORLD_ALSO=250,300,...`
y `VICTORY_DUMP_TARGETS=1` (NO por numero de frame absoluto: cambia entre corridas).

## Estado (2026-11-25)
- Con el fork: boot, menús, cinemática y prólogo renderizan en 3D, estable (~1 fps), sin cuelgues.
- **Las zonas en sombra salen negras:** el juego ilumina con ray tracing hecho en shaders compute y no corre.
  Faltan: 5 `CS_TraceShadowRays_*` (con `VICTORY_RT_STUB=1` traducen 4; el descriptor de la estructura se elige
  por índice dinámico y el host no da su dirección), el BVH builder (`VICTORY_BVH_BUILDER=1`, 3 de 52 sin
  pipeline en 32 lanes) y verificar que los targets de luz dejen de estar en cero.
- Otras pistas de upstream: issue #1317 (mundo negro en NVIDIA), #578/#189 (wave64 en 32 lanes).
