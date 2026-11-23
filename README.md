# Mania Renderer

osu!mania replay renderer: turns osu! stable replays (.osr) — or any mania map in Auto mode — into smooth 60 fps videos, drawn with osu! stable's skin rules, with movable overlays (KPS counter, hit error bars, hit counts, now playing, osukps, NPS chart...).

Renderizador de replays de osu!mania: convierte replays de osu! stable (.osr) — o cualquier mapa de mania en modo Auto — en videos fluidos a 60 fps, dibujados con las reglas de skins de osu! stable, con overlays movibles.

## What's in the package / Qué trae el paquete
- `mania-renderer.exe` — Windows: open it, nothing to install / ábrelo, no hay que instalar nada.
- `install-linux.sh` + `source/` — Linux: open a terminal in this folder and run `sh install-linux.sh` (it builds the program and adds it to your app menu) / abre una terminal en esta carpeta y ejecuta `sh install-linux.sh` (compila el programa y lo agrega al menú).
- `licencias/` — licences of the bundled skin and fonts / licencias de la skin y fuentes incluidas.

## Requirements / Requisitos

### Windows
- Windows 10 or 11, 64-bit / de 64 bits.
- 4-core CPU recommended / procesador de 4 núcleos recomendado (frames are drawn by the CPU / los cuadros se dibujan con el procesador).
- 4 GB RAM (8 GB for heavy storyboards / para storyboards pesados).
- ffmpeg: use the **Install ffmpeg** button in the program, or any ffmpeg on PATH / usa el botón **Instalar ffmpeg** o cualquier ffmpeg en el PATH.
- Graphics card optional / tarjeta gráfica opcional (only speeds up compression / solo acelera la compresión).
- osu! stable with its Songs folder (for replays) / osu! stable con su carpeta Songs (para replays).

### Linux
- Recent 64-bit distribution / distribución de 64 bits reciente (Ubuntu 22.04+, Debian 12+, Fedora, Arch, Kali...).
- Vulkan or OpenGL 3.3 (any GPU or Mesa software driver / cualquier tarjeta o el controlador de software Mesa).
- GTK 3 and ALSA / PulseAudio / PipeWire.
- Rust (to build / para compilar): https://rustup.rs
- Install / Instalar: `sh install-linux.sh`
- ffmpeg: **Install ffmpeg** button or your package manager / o tu gestor de paquetes.

## Build on Windows / Compilar en Windows
Requires Rust and Visual Studio Build Tools (C++). `cargo build --release` → `target/release/mania-renderer.exe`.

See `CREDITS.md` for credits and licences / Créditos y licencias en `CREDITS.md`.
