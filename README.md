# leonida-victory

A fork of [VIctory](https://github.com/Marvisak/VIctory), the compatibility layer that runs an Xbox Series title natively on a Windows PC from files the user supplies, built on [fxceless/victory-fork](https://github.com/fxceless/victory-fork). Maintained by **blacksi**.

This fork is about running it **well on NVIDIA** cards, Windows 10, and an ordinary PC: every compute program of the title's shader pack gets a pipeline on 32-lane waves, the first mission can be played to the end, and the frame time is brought down step by step.

**This repository contains no game files, code or assets.** You need your own copy of the title. It is not affiliated with or endorsed by the title's developer or publisher.

## Where it stands (2026-11-25)

One machine: NVIDIA GeForce RTX 5060 Ti 16 GB, Intel Core i7-11700, 32 GB RAM, Windows 10.

| | |
|---|---|
| Boot, menus, videos | 30 fps |
| Series S mode (1280x720, interface composed over it), in the world | about 135-160 ms a frame (6-7 fps) |
| Series X mode, in the world | about 5 fps |
| First mission (the boat) | completed, manual save; the boat floats and drives |
| Faces | the face network runs: mouths move when characters talk |
| Shader pack on NVIDIA | 14,247 of 14,247 vertex and pixel programs validate; 1,681 compute pipelines made, **0 refused** |

What is still wrong:

- Ray tracing does not run: shadows from rays are left out (`VICTORY_RT_STUB`) and rooms without direct light are dark.
- Rectangular dark patches on some water surfaces, and dark squares on some ground.
- Exposure and bloom can go too bright in some scenes.
- Memory: an unstable RAM overclock (an XMP profile on this machine) removed the GPU device within minutes; at stock speed the same build is stable.

## What is different here

- **Water physics** (boats float): textures that the ocean's compute passes write and the title's CPU code reads are copied back into title memory after each list (`VICTORY_TEXTURE_BACK`, after Geckomacabre/vx-layer), with the GFX10 XOR swizzle modes addressed so the height map can be tiled back (after PastiiJay/klbf-lab and MiamiGame/XTP, patterns from Mesa's addrlib).
- **Series S picture**: the present's planes are composed one over the other, as the console's display does, so the HUD, subtitles and map show over the 1280x720 world (`layers.rs`, after klbf-lab).
- **NVIDIA (32-lane waves)**: wave64 compute programs in the paired form (fxceless #854), and the NVIDIA fixes of fxceless #823, #799, #592, #615, #1006, #1112.
- **Shader translation**: signature rows hold one interpolation mode each, and the container lists elements in the module's order (+108 programs validate); 16-bit buffer accesses keep their bits (the face network, fxceless #927); typed stores marked; fixes from klbf-lab (NaN guards on colour outputs, first interpolation halves, f16 conversions).
- **Frame time**: the submitting thread no longer waits for the card when the title writes memory shaders are writing (`VICTORY_MERGE_RUNS`), large refreshes of title memory are cut into pieces and copied on several threads (`VICTORY_SPLIT_COPY`, `VICTORY_WIDE_REFRESH`), and `monitorx`/`mwaitx` are patched in place so they no longer trap.
- **Page write marks** (`VICTORY_VIEW_MARKS`, experimental): shaders note in a bitmap which 64 KiB pages they wrote, so copy-back could look only at those. Verified exact (0 misses in 2.5 million pages); skipping is off until it can be verified while skipping.
- **Watchdog** (`VICTORY_NO_WATCHDOG=1`): the title's own watchdog store to address 0 is stepped over, so a long frame or load no longer ends the run.
- **Performance overlay** (F10): frame rate, 30 s high/low/average, draws, CPU, GPU, RAM, VRAM (from Neptune, VIctory Edit.).
- **Diagnostics**: `VICTORY_TRACE_HASH` logs what a given program's views hold, and F11 captures can save the textures a traced program reads.

## How to run it

You need Windows 10 or 11 x64, a Direct3D 12 GPU with shader model 6.6 and mesh shaders (on Windows 10, the D3D12 Agility SDK runtime), Rust (the toolchain is pinned), the MSVC build tools, and your own dump of the title.

```powershell
git clone https://github.com/mauri9911/leonida-victory
cd leonida-victory
copy victory.example.toml victory.toml      # then set your paths, outside this folder
cargo build --profile diag -p victory
target\diag\victory.exe warm <path of your copy>\shaders.rpf
scripts\play.ps1 -Exe <path of your copy's executable> [-SeriesX] [-Agility <folder with D3D12Core.dll>]
```

## Credits

This fork stands on the work of many people; their code is used under the GPL-3.0 and keeps its authors in the history:

- [Marvisak/VIctory](https://github.com/Marvisak/VIctory) and its contributors.
- [fxceless/victory-fork](https://github.com/fxceless/victory-fork), and the pull requests merged from it: #203 (triippywhale), #854, #823, #799, #592, #615, #1006, #1112, #927, #868 and their authors.
- [myokex/victory-amd](https://github.com/myokex/victory-amd): texture spans, dispatches in parts, the early pipeline list.
- [Geckomacabre/vx-layer](https://github.com/Geckomacabre/vx-layer): the texture copy-back the boats float on.
- [PastiiJay/klbf-lab](https://github.com/PastiiJay/klbf-lab): XOR swizzle modes, plane composition, FidelityFX upscaling, shader fixes.
- [MiamiGame/XTP](https://github.com/MiamiGame/XTP) and Mesa's addrlib (MIT): the swizzle patterns.
- Neptune (VIctory Edit.), as published in [LooperSalty/AnyXboxX](https://github.com/LooperSalty/AnyXboxX): the performance overlay.

## License

GPL-3.0, as VIctory. See [LICENSE](LICENSE).
