#!/usr/bin/env bash
# Cross-builds the Windows exe on Linux and packs a Velopack installer into ./dist/releases.
# Needs: rustup target x86_64-pc-windows-gnu, x86_64-w64-mingw32-gcc, pdfium/win/bin/pdfium.dll
# (scripts/fetch-pdfium.sh --windows), .NET 8 SDK and `dotnet tool install -g vpk --version 1.2.161`.
set -euo pipefail
cd "$(dirname "$0")/.."

# User-local dotnet install (dotnet-install.sh) if there is no system one.
if ! command -v vpk >/dev/null && [[ -x "$HOME/.dotnet/tools/vpk" ]]; then
  export DOTNET_ROOT="$HOME/.dotnet"
  export PATH="$HOME/.dotnet:$HOME/.dotnet/tools:$PATH"
fi

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
target=x86_64-pc-windows-gnu
dll=pdfium/win/bin/pdfium.dll
[[ -f $dll ]] || { echo "missing $dll: run scripts/fetch-pdfium.sh --windows" >&2; exit 1; }

cargo build --release --target "$target"
exe=target/$target/release/simplepdf.exe
file "$exe"

pack=dist/pack
rm -rf "$pack"
mkdir -p "$pack" dist/releases
cp "$exe" "$dll" "$pack/"

# vpk only cross-packages for Windows when told so with the [win] directive.
vpk '[win]' pack --packId SimplePDF --packVersion "$version" --packTitle SimplePDF \
  --runtime win-x64 --packDir "$pack" --mainExe simplepdf.exe --outputDir dist/releases
ls -l dist/releases
