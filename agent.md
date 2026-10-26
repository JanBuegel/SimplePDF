# Building the Windows app (agent notes)

SimplePDF ships for Windows, but development happens on Linux. **Primary path: cross-build on Linux** with `scripts/package-windows.sh`; `CI.md` covers the toolchain and the GitHub Actions workflow. A Windows machine is only needed to run and test the app.

Host names below are placeholders. If `agent.local.md` exists (it is not committed), it maps them to real addresses, users and keys.

## Cross-build (Linux)

```sh
scripts/fetch-pdfium.sh --windows   # once: pdfium.dll -> pdfium/win/bin
scripts/package-windows.sh          # -> dist/releases/SimplePDF-win-Setup.exe, -Portable.zip, nupkg
```

Needs `rustup target add x86_64-pc-windows-gnu`, the MinGW cross-linker `x86_64-w64-mingw32-gcc` (wired up in `.cargo/config.toml`), the .NET 8 SDK and `vpk` 1.2.161, the same version as the `velopack` crate. The script also finds a user-local dotnet in `~/.dotnet`. Check the result with `file`: it must say `(GUI)`, otherwise a console window opens on the presentation PC.

## Native build (Windows, fallback)

On a Windows machine with Rust (GNU toolchain plus WinLibs MinGW, or MSVC), the .NET 8 SDK and `vpk`:

```powershell
.\scripts\fetch-pdfium.ps1
.\scripts\package-windows.ps1     # cargo build --release + vpk pack
```

Over SSH to `<build-vm>`, the shell is `cmd.exe`: chain commands with `&`/`&&`, not `;`, and use `powershell -NoProfile -Command "..."` for anything more. A plain SSH session doesn't have MinGW, dotnet or vpk on `PATH`; a small wrapper script that extends `PATH` and calls `package-windows.ps1` helps. Sync sources with tar, because `scp -r` with several sources has broken off midway:

```sh
tar czf - src Cargo.toml Cargo.lock | ssh <build-vm> "tar xzf - -C simplepdf"
```

## Rules

- **Bump the version for every installer you hand out.** Velopack doesn't update an installed copy to the same version. Add a German, user-facing entry to `CHANGELOG.md`.
- Before building, run `cargo fmt`, `cargo clippy --all-targets` and `cargo test`.
- Release builds use `windows_subsystem = "windows"`, so they have no console and `eprintln!` goes nowhere. Use `log::` instead. The log goes to `%LOCALAPPDATA%\SimplePDF\simplepdf.log`, appends across runs, includes panics, and rotates at 4 MB.
- Don't commit host addresses, user names, key paths or event details. They belong in `agent.local.md`.
- To check whether two builds match, compare hashes, not timestamps. VM clocks can be off around DST switches.

## Testing on the presentation PC

- Put test builds in a folder the PC can reach, together with `.cmd` launchers, and let the user start them. Don't start GUI apps or take screenshots in the user's session without explicit consent.
- `cmd` can't use a UNC path as its working directory and falls back to `C:\Windows`. So pass the PDF folder explicitly and use `start "" /d "%USERPROFILE%" ...`.
- Each launcher first kills a running `simplepdf.exe`, because the OSC port 9000 is single-use. Pass `--feedback <companion-host>:<port>` if Companion should see the status.
- Read the log over SSH: `type %LOCALAPPDATA%\SimplePDF\simplepdf.log`. Native crashes, such as one inside a driver DLL, appear only in the Windows event log:
  ```powershell
  Get-WinEvent -FilterHashtable @{LogName='Application'; StartTime=(Get-Date).AddHours(-3)} | ? { $_.Message -match 'simplepdf' }
  ```

## GPU switches (environment variables)

Each window has its own wgpu device. These variables override the defaults:

| Variable | Values | Default |
|---|---|---|
| `SIMPLEPDF_CONTROL_GPU` / `SIMPLEPDF_OUTPUT_GPU` | `low`, `high`, `none` | control `low`, output `high` |
| `SIMPLEPDF_CONTROL_BACKEND` / `SIMPLEPDF_OUTPUT_BACKEND` | `dx12`, `vulkan`, `gl` (comma list) | `dx12` on Windows |
| `SIMPLEPDF_PRESENT` | `fifo`, `mailbox`, `immediate`, `auto` | vsync (`AutoVsync`) |

What we've learned on laptops with two GPUs (integrated + discrete):
- Intel's Vulkan driver (`igvk64.dll` 31.0.101.2130) crashes with an access violation. That's why DX12 is the default on Windows.
- DX12 refuses to create a swap chain (`0x887A0001`) for a window created fullscreen. The output window therefore opens windowed and switches to fullscreen afterwards.
