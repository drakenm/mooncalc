#!/usr/bin/env bash
# Collect one release build into named archives and installers.
#
# Usage: tools/release/package.sh <linux|windows> <standard|de440> <label> <out-dir>
#
# Run from the repository root after
#   cargo build --release --locked -p mooncalc-gui [--features de440]
#   npm run tauri -- build --ci --bundles <...> [--features de440 --config ...]
# (see .github/workflows/release.yml). <label> is the version shown in file
# names, e.g. 0.1.1-rc1. Output names:
#   mooncalc-egui[-de440]-<label>-<os>-x86_64.(zip|tar.gz)
#   mooncalc-tauri[-de440]-<label>-windows-x86_64(-setup.exe|.msi)
#   mooncalc-tauri[-de440]-<label>-linux-x86_64(.deb|.AppImage)
set -euo pipefail

if [ "$#" -ne 4 ]; then
  echo "usage: $0 <linux|windows> <standard|de440> <label> <out-dir>" >&2
  exit 2
fi
os=$1 variant=$2 label=$3 out=$4

case "$os" in
  linux) exe="" ;;
  windows) exe=".exe" ;;
  *) echo "unknown os: $os" >&2; exit 2 ;;
esac
case "$variant" in
  standard) suffix="" ;;
  de440) suffix="-de440" ;;
  *) echo "unknown variant: $variant" >&2; exit 2 ;;
esac

release=target/release
bundle=$release/bundle
mkdir -p "$out"
out=$(cd "$out" && pwd)

# --- egui app: binary, licence and a short readme in one archive ---------
name="mooncalc-egui${suffix}-${label}-${os}-x86_64"
stage=$(mktemp -d)
mkdir "$stage/$name"
cp "$release/mooncalc${exe}" "$stage/$name/"
cp LICENSE "$stage/$name/LICENSE.txt"
{
  echo "MoonCalc ${label} (egui desktop app, ${os} x86_64)"
  echo
  echo "Run mooncalc${exe}. Everything it needs is built in: the Meeus/ELP"
  echo "ephemeris, the city atlas, coastlines and IANA time zones."
  if [ "$variant" = de440 ]; then
    echo
    echo "This build can also read JPL DE440 or DE440s ephemeris files. They are"
    echo "not included (de440s.bsp is about 32 MB). Download one from"
    echo "https://naif.jpl.nasa.gov/pub/naif/generic_kernels/spk/planets/"
    echo "and choose it under Ephemeris in the side panel."
  fi
  if [ "$os" = linux ]; then
    echo
    echo "Needs an X11 or Wayland desktop with OpenGL, plus fontconfig."
  fi
  echo
  echo "Source, documentation and attributions: https://github.com/drakenm/mooncalc"
} > "$stage/$name/README.txt"

if [ "$os" = windows ]; then
  # Built-in Windows tools only, so this also runs outside the CI image.
  (cd "$stage" && powershell.exe -NoProfile -NonInteractive -Command \
    "Compress-Archive -Path '$name' -DestinationPath '$name.zip' -Force")
  mv "$stage/$name.zip" "$out/"
else
  tar -C "$stage" -czf "$out/$name.tar.gz" "$name"
fi
rm -rf "$stage"

# --- Tauri app: rename the bundler's installers -----------------------------
# Exactly one file is expected per bundle format; anything else means stale
# output from an earlier build, so fail rather than guess.
take() { # <glob dir> <pattern> <destination>
  local found=()
  shopt -s nullglob
  found=("$1"/$2)
  shopt -u nullglob
  if [ "${#found[@]}" -ne 1 ]; then
    echo "expected one $2 in $1, found ${#found[@]}" >&2
    exit 1
  fi
  cp "${found[0]}" "$out/$3"
}

tauri="mooncalc-tauri${suffix}-${label}-${os}-x86_64"
if [ "$os" = windows ]; then
  take "$bundle/nsis" "*-setup.exe" "$tauri-setup.exe"
  take "$bundle/msi" "*.msi" "$tauri.msi"
else
  take "$bundle/deb" "*.deb" "$tauri.deb"
  take "$bundle/appimage" "*.AppImage" "$tauri.AppImage"
  chmod +x "$out/$tauri.AppImage"
fi

ls -l "$out"
