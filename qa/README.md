# Conversion checks

`python qa/security.py` exercises resource rejection, embedded-image preservation, a rooted Typst PDF step, oversized PDF/SVG rejection and failed-output cleanup through the real CLI. It starts a loopback HTTP fixture and verifies that blocked document references make no requests. Run with the helper build dependencies installed and the newly built application and helper in place.

The development Windows build passed 105 real-output conversion checks and 11 Rust tests. The corpus checks decoded content and structure, not just output existence. Results are local evidence, not a guarantee for every codec or document.

## Rust checks

```powershell
cargo fmt -- --check
cargo test
```

Tests cover output collision handling, cancellation before publication, cancellation of an FFmpeg process, structured-data preservation, literal CSV-to-Excel cells, multi-sheet exports, quality mapping, filename timing and fullscreen geometry. The running-process cancellation test skips when FFmpeg is unavailable.

## Integration corpus

Install the complete engines described in [the development guide](../docs/DEVELOPMENT.md), including the packaged PDF helper and LibreOffice. Put FFmpeg/ffprobe on PATH as well as making them discoverable to Dropflip.

Create a separate test environment, generate synthetic fixtures, then run:

```powershell
python -m venv .tools/qa-venv
.\.tools\qa-venv\Scripts\python.exe -m pip install -r qa/requirements.txt
.\.tools\qa-venv\Scripts\python.exe qa/layout_fixture.py
.\.tools\qa-venv\Scripts\python.exe qa/smoke.py
.\.tools\qa-venv\Scripts\python.exe qa/extended.py
```

Both runners default to `target/release/dropflip.exe`; pass an executable path as the first argument to test a different build. Build the release binary before running them.

`smoke.py` covers image/document/data/media outputs, invalid data, collision handling, OCR, PDF layout/slides and quality behavior. `extended.py` adds EXIF orientation, transparency, lossless 16-bit PNG, complete page exports, streaming animated WebP frames/timing, typed spreadsheet values, multiple sheets, embedded resources, legacy Office formats, subtitle/audio/attachment counts, and 24-bit audio.

`extended.py` can run one section by setting `DROPFLIP_QA_SECTION` to `images`, `sheets`, `documents` or `media`. Fresh fixtures may add a setup conversion to the count. The fixture generator uses Windows Arial fonts and ReportLab to create a two-page digital PDF, a scanned PDF, and an image; it does not use personal documents.

Reports and generated outputs stay under the ignored `qa/` paths. A failed assertion stops the runner. The extended runner saves progress records after each conversion; a report by itself is not a passing result unless the runner finished successfully.

## Optional HEIC fixture

The image section also tests `qa/fixtures/libheif-example.heic` if present. The development input came from [libheif's example](https://github.com/strukturag/libheif/blob/master/examples/example.heic), SHA-256 `7f8b363e4936c0666a25f64f3a92fda10bd8e5453be4592530b65a55dd98f3f2`. This third-party fixture is not committed here.

## Native UI probes

Debug builds support the development hooks `DROPFORM_UI_PROBE`, `DROPFORM_UI_PROBE_MODE`, and `DROPFORM_CAPTURE_PATH`. Modes include `picker`, `quality-memory`, `settings-flow` and `completion`. They feed the app's own egui widgets and write isolated reports; they do not automate a Windows folder picker or prove OS-level OLE drag delivery.

Use `DROPFLIP_CONFIG` to isolate test settings. Never point a test at an output directory containing files you need to preserve; use the runners' generated output folders. Exclusive fullscreen games and cross-monitor drop placement are outside the current validated scope.
