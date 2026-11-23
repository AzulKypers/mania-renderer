#!/bin/sh
set -e
root="$(cd "$(dirname "$0")" && pwd)"
if [ -f "$root/source/Cargo.toml" ]; then
    src="$root/source"
else
    src="$root"
fi
cd "$src"

if ! command -v cargo >/dev/null 2>&1; then
    echo "Rust is not installed. Install it with: curl https://sh.rustup.rs -sSf | sh"
    echo "Then open a new terminal and run this script again."
    exit 1
fi

missing=""
for lib in gtk+-3.0 alsa; do
    if command -v pkg-config >/dev/null 2>&1 && ! pkg-config --exists "$lib"; then
        missing="$missing $lib"
    fi
done
if [ -n "$missing" ]; then
    echo "Missing development libraries:$missing"
    echo "  Debian/Ubuntu/Kali: sudo apt install build-essential pkg-config libgtk-3-dev libasound2-dev libxkbcommon-dev"
    echo "  Fedora:             sudo dnf install gcc pkgconf-pkg-config gtk3-devel alsa-lib-devel libxkbcommon-devel"
    echo "  Arch:               sudo pacman -S base-devel pkgconf gtk3 alsa-lib libxkbcommon"
    exit 1
fi

echo "Building Mania Renderer..."
echo "This takes a while. The last step (mania-renderer(bin)) can take 5 to 20 minutes with no new output: it is not stuck, please wait."
cargo build --release

bin="$src/target/release/mania-renderer"
rm -f "$HOME/.local/bin/mania-render" "$HOME/.local/share/applications/mania-render.desktop" "$HOME/.local/share/icons/mania-render.png"
mkdir -p "$HOME/.local/bin" "$HOME/.local/share/applications" "$HOME/.local/share/icons"
"$bin" --export-icon "$HOME/.local/share/icons/mania-renderer.png"
ln -sf "$bin" "$HOME/.local/bin/mania-renderer"
cat > "$HOME/.local/share/applications/mania-renderer.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Mania Renderer
Comment=osu!mania replay renderer
Exec=$bin
Icon=$HOME/.local/share/icons/mania-renderer.png
Terminal=false
Categories=Game;AudioVideo;
DESKTOP

echo "Done. Open it from your application menu or run: mania-renderer"
if ! command -v ffmpeg >/dev/null 2>&1; then
    echo "ffmpeg was not found: use the \"Install ffmpeg\" button inside the program."
fi
