# Conversion engines and libraries

Dropflip's Rust source is under MIT. Engines remain separate executables with their own licenses. The app icon's frame treatment adapts the adjacent MIT-licensed Dictately generator; its notice is in assets/DICTATELY-LICENSE.txt. Dropflip's circular-arrow glyph is newly drawn geometry.

| Engine | Version used for local validation | License / source |
| --- | --- | --- |
| FFmpeg and ffprobe | Gyan FFmpeg 9.0.1 full build, copied from the existing Windows installation | See tools/licenses/FFmpeg-LICENSE and FFmpeg-README.txt; https://www.gyan.dev/ffmpeg/builds/ and https://ffmpeg.org/ |
| Pandoc | 3.12.1 | GPL; tools/licenses/Pandoc-COPYING.rtf; https://github.com/jgm/pandoc |
| Typst | 0.15.1 | Apache-2.0; tools/licenses/Typst-LICENSE; https://github.com/typst/typst |
| pdf2docx | 0.5.13 | MIT; tools/licenses/pdf2docx-LICENSE; https://github.com/ArtifexSoftware/pdf2docx |
| PyMuPDF / MuPDF | 1.26.7 | AGPL-3.0 or commercial; tools/licenses/PyMuPDF-COPYING; https://pymupdf.readthedocs.io/ |
| LibreOffice | 26.2.6, portable administrative extraction | MPL-2.0 and bundled component licenses; tools/office/LICENSE.html and NOTICE; https://www.libreoffice.org/ |
| Tesseract fast language data | pinned English and Indonesian | Apache-2.0; engines/tessdata-manifest.json; https://github.com/tesseract-ocr/tessdata_fast |
| python-pptx | 1.0.2 | MIT; tools/licenses/python-pptx-LICENSE; https://python-pptx.readthedocs.io/ |

The engine binaries, `tools/licenses/` copies and local runtime manifests are not included in this source repository. Obtain and retain the relevant licenses when installing those engines. `scripts/fetch-engines.mjs` pins and verifies the Pandoc/Typst archives; normal source builds cache them under `.tools/downloads/`. The PDF helper packages Python, python-docx, python-pptx, Pillow, XlsxWriter, NumPy, OpenCV, fonttools and lxml; pinned build dependencies are in `engines/requirements.txt`. `scripts/build-word-engine.ps1` builds the helper and verifies OCR data hashes. Tkinter is excluded. Preserve engine notices and meet their corresponding-source requirements before redistributing a runtime bundle.

Rust dependency versions are pinned in Cargo.lock. Principal libraries are eframe/egui (MIT/Apache-2.0), image/tiff (MIT/Apache-2.0), lopdf (MIT), rfd (MIT), tray-icon/muda (MIT/Apache-2.0), serde and serde_json (MIT/Apache-2.0), csv (MIT/Unlicense), calamine 0.37.0 (MIT), rust_xlsxwriter 0.101.0 (MIT/Apache-2.0), zip (MIT), tempfile (MIT/Apache-2.0), and directories (MIT/Apache-2.0). See each resolved crate's LICENSE files for full terms. The HEIC QA fixture comes from https://github.com/strukturag/libheif/blob/master/examples/example.heic (SHA-256 7F8B363E4936C0666A25F64F3A92FDA10BD8E5453BE4592530B65A55DD98F3F2); it is a test input, not a bundled runtime asset.
