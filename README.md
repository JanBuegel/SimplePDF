# SimplePDF

**A presenter view for PDFs.** Show slides fullscreen on the projector while the laptop screen shows a live preview, the next page and an overview of all pages. Control it with the keyboard, a presenter clicker, or over the network via OSC, e.g. from a Bitfocus Companion / Stream Deck.

[![Latest release](https://img.shields.io/github/v/release/JanBuegel/SimplePDF?label=download)](https://github.com/JanBuegel/SimplePDF/releases/latest)

- **Two windows:** a control window on your screen, a fullscreen output on the external display.
- **Two output modes:** *Vollbild* (current page) and *Übersicht* (all pages, click one to jump there), plus a black screen.
- **Folder-based:** point it at a folder of PDFs and switch talks with one click, one key or one OSC message.
- **Remote control:** a complete OSC interface with status feedback, so buttons can light up for the current file, mode and black screen.
- **Clicker-proof:** on Windows, page keys keep working even when another program has focus.
- **Per-file image adjustment:** crop margins or stretch to the output, e.g. for widescreen slides exported onto A4 paper.

The UI labels are German (*Vollbild*, *Übersicht*, *Schwarz* …); this README explains them.

## Install (Windows)

Download from the [latest release](https://github.com/JanBuegel/SimplePDF/releases/latest):

| File | Use |
|---|---|
| `SimplePDF-win-Setup.exe` | Installer. It installs per user to `%LocalAppData%\SimplePDF`, needs no admin rights, and creates Start-menu and desktop shortcuts. Installing a newer version updates the existing one. |
| `SimplePDF-win-Portable.zip` | No installation: unzip and start `SimplePDF.exe`. |

The builds are not code-signed, so Windows SmartScreen may warn on first start: choose *Weitere Informationen → Trotzdem ausführen* (*More info → Run anyway*).

On first start, Windows asks whether SimplePDF may use the network. Allow it if you want to control it from another device (see [Network](#network-and-osc)).

## Quick start

1. Start SimplePDF and drag a folder of PDFs (or a single PDF) onto the window. Alternatively, start it with the folder as argument: `simplepdf.exe "D:\Talks"`.
2. Click a file in the list.
3. Press `F5` to open the output on the external display. It goes to a monitor that isn't showing the control window; pick another one in the toolbar if needed.
4. Turn pages with the arrow keys, PageUp/PageDown or a presenter clicker. `Esc` closes the output.

Name files `01_…`, `02_…` to fix their order: the list is sorted by name, and remote buttons select files by position.

## The windows

**Control window** (your screen):
- file list of the current folder
- live preview of what the output shows
- the next page
- grid of all pages; click one to jump to it
- toolbar: output on/off, mode, black screen, output monitor, *Clicker global*, image adjustment

**Output window** (fullscreen, external display):
- **Vollbild:** the current page fills the screen.
- **Übersicht:** all pages in a grid with the current one highlighted. Clicking a page jumps to it and switches to Vollbild.
- **Schwarz:** blanks the output.

With only one display, the output covers the control window; toggle it with `F`.

### Image adjustment

**Bildanpassung (diese Datei)** in the toolbar changes how the current PDF is shown on the output and in the preview:
- **Ränder beschneiden …** crops left/right/top/bottom in percent. **A4 quer → 16:9** removes the top and bottom margins of 16:9 slides that were exported onto landscape A4 paper.
- **Auf Ausgabe strecken** fills the whole output; the aspect ratio may change.
- **Zurücksetzen** restores the original page.

Adjustments apply to every page of that PDF and are remembered per file until the app closes.

## Keyboard and clicker

| Key | Action |
|---|---|
| → ↓ PageDown Space Enter | next page |
| ← ↑ PageUp Backspace | previous page |
| Home / End | first / last page |
| O | toggle Vollbild / Übersicht |
| B or `.` | black screen |
| F5 / Esc / F | output on / off / toggle |
| `]` / `[` | next / previous file |

Page keys never switch files: at the last page, "next" does nothing. Presenter clickers send these keys, including `B`/`.` for their black-screen button.

**Clicker global** (Windows, on by default): while the output is running, PageUp/PageDown and ←/→ turn pages even when another program has focus. While that's active, these keys don't reach other programs. When the output stops, they're released again. If another program has already claimed a key, the toolbar says so.

## Network and OSC

SimplePDF listens for [OSC](https://opensoundcontrol.stanford.edu/) messages on **UDP port 9000, on all network interfaces** (`0.0.0.0`). It works on any network: LAN, Wi-Fi, a direct cable or a VPN. To restrict it, use `--osc-bind` with the address of one interface (e.g. `--osc-bind 127.0.0.1` for local control only). To change the port, use `--osc-port`.

When controlling from another device, allow inbound UDP on that port in the firewall. Windows asks on first start.

The `/pdf` prefix is optional, and addresses are case-insensitive. On trigger addresses (marked ✱), a first argument of `0` is ignored, so a button can send `1` on press and `0` on release without firing twice.

| Address | Argument | Action |
|---|---|---|
| `/pdf/next` ✱ | | next page |
| `/pdf/prev` ✱ | | previous page |
| `/pdf/first` ✱, `/pdf/last` ✱ | | first / last page |
| `/pdf/page` | int (1-based) | go to page |
| `/pdf/mode` | `"fullscreen"`/`"overview"` or `0`/`1`; none = toggle | set mode |
| `/pdf/mode/toggle` ✱ | | toggle mode |
| `/pdf/mode/fullscreen` ✱, `/pdf/mode/overview` ✱ | | set mode |
| `/pdf/black` | `0`/`1`; none = toggle | black screen |
| `/pdf/black/toggle` ✱ | | toggle black screen |
| `/pdf/output` | `0`/`1`; none = toggle | output window off/on |
| `/pdf/output/toggle` ✱ | | toggle output window |
| `/pdf/output/monitor` | string (name) or `"auto"` | output monitor |
| `/pdf/file/select` | int (1-based, position in list) | open file |
| `/pdf/file/open` | string | open by path or file name (case-insensitive; exact match, then start of the name, then any part of it) |
| `/pdf/file/next` ✱, `/pdf/file/prev` ✱ | | next / previous file (wraps around) |
| `/pdf/folder` | string | switch folder |
| `/pdf/rescan` ✱ | | re-read folder |

The folder is re-read before every file selection, so files copied in during a show appear. Page commands sent while a file is still loading are applied once it's open, so `file/select 3` followed by `page 5` works.

### Status feedback

With `--feedback HOST:PORT`, SimplePDF sends its state to that address whenever it changes (only the changed values):

| Address | Type |
|---|---|
| `/pdf/status/page` | int (1-based, 0 = no file) |
| `/pdf/status/pages` | int |
| `/pdf/status/file` | string |
| `/pdf/status/file_index` | int (1-based) |
| `/pdf/status/files` | int |
| `/pdf/status/mode` | `"fullscreen"` / `"overview"` |
| `/pdf/status/black` | int 0/1 |
| `/pdf/status/output` | int 0/1 |

`HOST` can be an IP address or a host name.

### Bitfocus Companion

SimplePDF needs no Companion module. The built-in **Generic: OSC** connection is enough.

**Ready-made page:**

```sh
scripts/companion-page.py --host <ip-of-the-simplepdf-pc>   # -> simplepdf.companionconfig
```

Import it in Companion via *Import / Export → Import*. It contains the connection and a page of buttons: previous/next, first/last, Vollbild/Übersicht, black screen, files 1–7, previous/next file, rescan and output on/off. Buttons light up from the status feedback. For that, start SimplePDF with `--feedback <ip-of-the-companion-pc>:9001`.

**Manual setup:**

1. Add a **Generic: OSC** connection with the SimplePDF PC as target host and port 9000.
2. Optionally, enable feedback listening in that connection and start SimplePDF with `--feedback <companion-ip>:<listen-port>`.
3. Create buttons with the action *Send message without arguments* and paths like `/pdf/next`. Use *Send integer* or *Send string* for `/pdf/page`, `/pdf/file/select` and `/pdf/file/open`.

A Windows shortcut with arguments is the easiest way to start SimplePDF with the right folder and feedback address:

```
"%LocalAppData%\SimplePDF\current\simplepdf.exe" "D:\Talks" --feedback 10.0.0.20:9001
```

## Command line

```
simplepdf [PATH] [--osc-port 9000] [--osc-bind 0.0.0.0] [--feedback HOST:PORT]
                 [--monitor NAME] [--present] [--overview]
```

| Option | Meaning |
|---|---|
| `PATH` | A folder, or a PDF (its folder becomes the file list). Default: the working directory. |
| `--osc-port` | UDP port for OSC commands (default 9000). |
| `--osc-bind` | Interface address to listen on (default `0.0.0.0` = all). |
| `--feedback HOST:PORT` | Send status updates via OSC. |
| `--monitor NAME` | Output monitor, as shown in the toolbar dropdown (e.g. `\\.\DISPLAY2`). Default: automatic. |
| `--present` | Open the output right away. |
| `--overview` | Start in Übersicht mode. |

## Troubleshooting

**Log file:** `%LocalAppData%\SimplePDF\simplepdf.log` (Linux/macOS: `simplepdf.log` in the temp folder). It records the chosen GPUs, monitors, dropped frames and crashes.

**Port 9000 already in use:** another SimplePDF is still running, or another program uses the port. Close it, or use `--osc-port`.

**Graphics problems (laptops with two GPUs):** each window renders on its own GPU. By default, the control window uses the power-saving GPU (usually the one driving the internal display) and the output uses the high-performance GPU, both with DirectX 12 on Windows. Environment variables override this:

| Variable | Values |
|---|---|
| `SIMPLEPDF_CONTROL_GPU`, `SIMPLEPDF_OUTPUT_GPU` | `low`, `high`, `none` |
| `SIMPLEPDF_CONTROL_BACKEND`, `SIMPLEPDF_OUTPUT_BACKEND` | `dx12`, `vulkan`, `gl` |
| `SIMPLEPDF_PRESENT` | `fifo`, `mailbox`, `immediate`, `auto` |

## Building

Needs Rust (stable) and the [Pdfium](https://github.com/bblanchon/pdfium-binaries) library. The app looks for Pdfium next to the executable, in `pdfium/bin` or `pdfium/lib`, in the working directory, and on the system path.

**Run on Linux/macOS:**

```sh
scripts/fetch-pdfium.sh          # libpdfium -> pdfium/lib
cargo run --release -- ~/Talks
```

**Windows installer, cross-built on Linux** (no Windows machine needed):

```sh
rustup target add x86_64-pc-windows-gnu
sudo dnf install mingw64-gcc mingw64-winpthreads-static   # Debian/Ubuntu: gcc-mingw-w64-x86-64
dotnet tool install -g vpk --version 1.2.161             # needs the .NET 8 SDK
scripts/fetch-pdfium.sh --windows                        # pdfium.dll -> pdfium/win/bin
scripts/package-windows.sh                               # -> dist/releases/
```

**Windows installer, built on Windows** (MSVC, or the smaller GNU toolchain plus [WinLibs](https://winlibs.com/) MinGW):

```powershell
.\scripts\fetch-pdfium.ps1         # pdfium.dll -> pdfium\bin and target\{debug,release}
dotnet tool install -g vpk         # once; needs the .NET 8 SDK
.\scripts\package-windows.ps1      # -> dist\releases\SimplePDF-win-Setup.exe (+ Portable.zip)
```

The version comes from `Cargo.toml`. Release builds on Windows have no console window. Without the installer, put `simplepdf.exe` and `pdfium.dll` in the same folder.

`CI.md` describes the GitHub Actions workflow, `CHANGELOG.md` lists the changes per version.
