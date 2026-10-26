# CI: Windows build

The Windows build is a cross-build: it runs on Linux and needs no Windows machine. The same steps work locally (`scripts/package-windows.sh`) and in GitHub Actions. Nothing is code-signed; Velopack's `--signParams` would need `signtool.exe` on Windows, and we don't use it.

## What the build needs

| Piece | Local (Fedora) | GitHub Actions (ubuntu-latest) |
|---|---|---|
| Rust target `x86_64-pc-windows-gnu` | `rustup target add x86_64-pc-windows-gnu` | same |
| MinGW cross-linker `x86_64-w64-mingw32-gcc` | `dnf install mingw64-gcc mingw64-winpthreads-static` | `apt-get install gcc-mingw-w64-x86-64` |
| .NET 8 SDK | `~/.dotnet` via dotnet-install.sh, or `dnf install dotnet-sdk-8.0` | `actions/setup-dotnet@v4` |
| `vpk` **1.2.161** (must match the `velopack` crate version in `Cargo.toml`) | `dotnet tool install -g vpk --version 1.2.161` | same |
| `pdfium.dll` (x64) | `scripts/fetch-pdfium.sh --windows` | same |

`.cargo/config.toml` points the GNU target at the cross-linker:

```toml
[target.x86_64-pc-windows-gnu]
linker = "x86_64-w64-mingw32-gcc"
```

## Steps

1. `cargo build --release --target x86_64-pc-windows-gnu` → `target/x86_64-pc-windows-gnu/release/simplepdf.exe`
2. Stage `simplepdf.exe` + `pdfium/win/bin/pdfium.dll` in `dist/pack/`.
3. `vpk [win] pack --packId SimplePDF --packVersion <Cargo.toml version> --packTitle SimplePDF --runtime win-x64 --packDir dist/pack --mainExe simplepdf.exe --outputDir dist/releases`
4. Outputs in `dist/releases/`: `SimplePDF-win-Setup.exe`, `SimplePDF-win-Portable.zip`, `SimplePDF-<ver>-full.nupkg` (+ `-delta.nupkg` when the previous release is present in `outputDir`).

Delta packages only appear if the previous `-full.nupkg` is in `dist/releases/` when `vpk` runs. In CI that means downloading the last release first (`vpk download github …`) or accepting full packages only.

## Workflow

Save as `.github/workflows/windows.yml`. It builds on every push and PR, and attaches the installer to a GitHub release when a `v*` tag is pushed.

```yaml
name: Windows build

on:
  push:
    branches: [main]
    tags: ["v*"]
  pull_request:

env:
  CARGO_TERM_COLOR: always
  VPK_VERSION: "1.2.161"

jobs:
  windows:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Rust toolchain
        uses: dtolnay/rust-toolchain@stable
        with:
          targets: x86_64-pc-windows-gnu
          components: clippy, rustfmt

      - name: MinGW cross-linker
        run: sudo apt-get update && sudo apt-get install -y gcc-mingw-w64-x86-64

      - uses: actions/setup-dotnet@v4
        with:
          dotnet-version: "8.0.x"

      - name: vpk
        run: dotnet tool install -g vpk --version "$VPK_VERSION"

      - uses: Swatinem/rust-cache@v2

      - name: Lint and test (host)
        run: |
          cargo fmt --check
          cargo clippy --all-targets -- -D warnings
          cargo test

      - name: pdfium.dll
        run: scripts/fetch-pdfium.sh --windows

      - name: Build and package
        run: scripts/package-windows.sh

      - uses: actions/upload-artifact@v4
        with:
          name: SimplePDF-windows
          path: |
            dist/releases/SimplePDF-win-Setup.exe
            dist/releases/SimplePDF-win-Portable.zip
            dist/releases/*.nupkg
            dist/releases/releases.win.json
            dist/releases/RELEASES

      - name: Release
        if: startsWith(github.ref, 'refs/tags/v')
        uses: softprops/action-gh-release@v2
        with:
          files: |
            dist/releases/SimplePDF-win-Setup.exe
            dist/releases/SimplePDF-win-Portable.zip
            dist/releases/*.nupkg
            dist/releases/releases.win.json
            dist/releases/RELEASES
```

## Conventions

- **Version:** `Cargo.toml` is the single source. Bump it before tagging; the tag must be `v<version>`. Velopack won't update an installed copy to the same version.
- **Changelog:** add an entry to `CHANGELOG.md` for every version that ships.
- **Cache:** `Swatinem/rust-cache` keys on `Cargo.lock`, so a dependency bump means a cold build (~5 min for this crate on a runner).
- **Auto-updates later:** the `releases.win.json`/`RELEASES` files uploaded to the GitHub release are what `velopack`'s `UpdateManager` reads. Keep uploading them even before the app checks for updates.

## Sanity check for a cross-built exe

`file dist/pack/simplepdf.exe` should say `PE32+ executable (GUI) x86-64, for MS Windows`. `(GUI)` confirms `windows_subsystem = "windows"` took effect; `(console)` means a console window would open on the presentation PC.
