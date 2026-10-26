#!/usr/bin/env bash
# Downloads a prebuilt Pdfium (bblanchon/pdfium-binaries).
#   fetch-pdfium.sh            -> libpdfium for this machine into ./pdfium/lib
#   fetch-pdfium.sh --windows  -> pdfium.dll (x64) into ./pdfium/win/bin, for the cross-build
set -euo pipefail
cd "$(dirname "$0")/.."
if [[ "${1:-}" == "--windows" ]]; then
  asset=pdfium-win-x64.tgz
  dest=pdfium/win
else
  case "$(uname -s)-$(uname -m)" in
    Linux-x86_64)  asset=pdfium-linux-x64.tgz ;;
    Linux-aarch64) asset=pdfium-linux-arm64.tgz ;;
    Darwin-arm64)  asset=pdfium-mac-arm64.tgz ;;
    Darwin-x86_64) asset=pdfium-mac-x64.tgz ;;
    *) echo "unsupported platform: $(uname -s)-$(uname -m)" >&2; exit 1 ;;
  esac
  dest=pdfium
fi
mkdir -p "$dest"
curl -fL "https://github.com/bblanchon/pdfium-binaries/releases/latest/download/$asset" | tar -xz -C "$dest"
ls -l "$dest"/lib "$dest"/bin 2>/dev/null || true
