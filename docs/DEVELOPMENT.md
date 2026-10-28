# Building Dropflip

## Requirements

- Windows x64. The current native build was validated on Windows 11.
- A stable Rust toolchain and Windows C/C++ build tools. MSVC builds need `rc.exe` on PATH; use Developer PowerShell for Visual Studio. GNU builds need `windres`.
- Python 3.12 for building the optional image/PDF helper.
- Node.js for the pinned Pandoc/Typst download script.
- FFmpeg and ffprobe for media formats; LibreOffice for Office layout and legacy formats.

The repository contains source and generated icon assets. It excludes executables, runtime libraries, trained OCR data, build caches and local QA outputs.

## Build the native application

```powershell
git clone https://github.com/AlthafTM/dropflip.git
cd dropflip
cargo build --release
Copy-Item target/release/dropflip.exe Dropflip.exe
```

Close a running Dropflip through its tray menu before replacing its executable. `scripts/cargo.ps1` is an optional wrapper around Cargo; a normal Rust installation can use `cargo` directly.

The GNU target configuration in `.cargo/config.toml` uses Rust's bundled linker. The default MSVC target does not use that target-specific block.

## Install conversion engines

The native application can handle a subset of image/data/spreadsheet formats without external engines. Install the following components for the broader format list.

### Pandoc and Typst

```powershell
.\scripts\setup-engines.ps1
```

This downloads pinned Windows archives from their official GitHub releases, verifies SHA-256 checksums, and copies `pandoc.exe` and `typst.exe` into `tools/`. Downloads remain in the ignored `.tools/downloads/` cache.

### FFmpeg and ffprobe

Install a Windows FFmpeg build with the software codecs used by the app, including libx264, libvpx-vp9, libopus, libvorbis, libmp3lame and libwebp. Put `ffmpeg.exe` and `ffprobe.exe` directly in `tools/` or on PATH. The development corpus used the Gyan full build recorded in [THIRD_PARTY.md](../THIRD_PARTY.md).

Codec availability varies between FFmpeg builds. A minimal/system build may not provide every advertised media encoder.

### LibreOffice

Install LibreOffice normally, or place a complete portable installation under `tools/office/` so `tools/office/program/soffice.com` and its supporting files exist. A standalone copy of the executable is insufficient.

### Image/PDF helper

```powershell
.\scripts\build-word-engine.ps1 -Python python
```

The script creates `.tools/pdf-word-venv/`, installs the pinned packages in `engines/requirements.txt`, fetches and verifies English/Indonesian OCR data, and builds `tools/pdf-word/` with PyInstaller. Keep this entire directory beside the application; its executable depends on `_internal/`.

Despite the historical executable name `pdf-word.exe`, the helper also supplies page images, SVG rendering, AVIF, PDF tables, OCR text, and PDF slides. Only the Word layout path imports OpenCV/NumPy.

ImageMagick is optional for additional image decoders. Poppler's `pdftoppm` supplies a fallback single-page PDF renderer when the packaged helper is absent.

### Tool discovery

The application looks in `DROPFORM_TOOLS`, the executable's `tools/` directory, the source checkout's `tools/` directory, and PATH. It also checks the standard Windows LibreOffice installation location.

Example layout:

```text
Dropflip.exe
tools/
  ffmpeg.exe
  ffprobe.exe
  pandoc.exe
  typst.exe
  pdf-word/
    pdf-word.exe
    _internal/
  office/
    program/
      soffice.com
      ... supporting installation files ...
```

## Run and start at sign-in

Open `Dropflip.exe`, then use its tray icon or a drop area. `Dropflip.exe --settings` opens Settings directly.

The source application does not create a Startup shortcut automatically. To enable sign-in startup for your own build, create a per-user shortcut targeting the executable's permanent location:

```powershell
$appPath = (Resolve-Path .\Dropflip.exe).Path
$startupPath = Join-Path ([Environment]::GetFolderPath('Startup')) 'Dropflip.lnk'
$shell = New-Object -ComObject WScript.Shell
$shortcut = $shell.CreateShortcut($startupPath)
$shortcut.TargetPath = $appPath
$shortcut.WorkingDirectory = Split-Path -Parent $appPath
$shortcut.IconLocation = "$appPath,0"
$shortcut.Save()
```

Settings use Dropflip's per-user Windows config directory. `DROPFLIP_CONFIG` selects an isolated settings file for development/tests; the legacy `DROPFORM_CONFIG` override also works. Existing legacy Dropform settings can migrate on first launch. Downloads is the initial output destination.

## Command-line interface

```text
Dropflip.exe --engines
Dropflip.exe --inspect INPUT
Dropflip.exe --convert INPUT FORMAT OUTPUT_FOLDER [--compress] [--compression 1-100]
```

Commands emit JSON. Pipe or redirect stdout when using the Windows GUI executable from a shell. `--inspect` returns the format identifiers supported for that input, including collection identifiers such as `png.zip`, `csv.zip` and `md.zip`.

CLI compression strength increases from 1 to 100. The UI uses a quality percentage instead: 100% leaves compression off; lower values increase compression. PNG/FLAC/ZIP use lossless compression effort, while lossy encoders reduce quality. Neither control promises an exact output size.

## Tests and implementation

See [QA instructions](../qa/README.md). The Rust modules cover the UI, native platform behavior, settings, conversion routing, image/data operations and spreadsheet IO. `engines/` holds the supervised PDF/image helper sources. Work occurs off the UI thread; conversions publish a staged result only after success.

The UI has no browser runtime. It paints a cached nine-slice shell and cached glow texture. Idle state requests no continuous redraw. The drag watcher polls while checking for a held drag, and a separate cadence in that worker detects foreground fullscreen changes. Preview generation was removed to reduce file-open work.

## Distribution

This source import is not a runtime installer. A usable portable build needs the engines required by its advertised formats and their supporting files. Preserve their notices and satisfy their redistribution/source obligations; see [THIRD_PARTY.md](../THIRD_PARTY.md). The MIT license for Dropflip's own source does not replace those licenses.
