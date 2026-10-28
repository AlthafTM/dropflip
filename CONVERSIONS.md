# Conversion behavior and verification

This build uses real decoders, encoders and document writers. It does not rename extensions or export only the first page of a multi-page document. Source files remain unchanged; failed/cancelled jobs publish no result. Some destination formats cannot represent everything in the source. The picker filters known incompatibilities, and the routes below describe remaining limits.

| Input / destination | What is preserved | Limits |
| --- | --- | --- |
| PNG/JPEG/TIFF/WebP/BMP/etc. → raster images | Image dimensions, orientation, pixels at the target's supported precision; alpha where supported | JPEG flattens onto white. GIF is indexed color. ICO is limited to 256 pixels. 16-bit PNG compression remains 16-bit and lossless. HDR is offered as floating-point TIFF; SDR tone mapping is not implemented. |
| HEIC/HEIF → common raster images | Primary image decoded by FFmpeg | Auxiliary depth/live-photo data are not converted into editable image layers. HEIC decoding was checked with libheif's example fixture. |
| AVIF ↔ common images | Dimensions and transparency; native AVIF encoding | AVIF/JPEG quality is lossy. The quality slider maps directly to their encoder quality. |
| SVG → PNG / PDF | Raster appearance for PNG; vector drawing paths for PDF | Fonts/external resources depend on the renderer. Does not trace raster pictures into editable vectors. |
| Multi-page TIFF → PDF / PNG pages / JPEG pages | Every page, in order | Image sets are ZIP files with numbered page names. |
| Animated WebP/GIF → video/GIF | Complete animation; frame timing via a lossless APNG intermediate for WebP | GIF has palette/timing limits and no audio. The WebP intermediate writes one frame at a time rather than holding the animation in RAM. |
| PDF → PNG/JPEG/SVG | Every page | Single-page PDFs produce a single image. Multi-page PDFs produce a numbered pages ZIP. Scanned artwork remains raster content inside SVG. |
| PDF → TXT | All page text; English/Indonesian OCR when a scanned page has no text layer | OCR can misread poor scans; no text means an error, rather than a misleading empty file. |
| PDF → DOCX | Reconstructed paragraphs, tables, pictures, columns; automatic OCR | PDF has no original Word layout model. Complex layouts can reflow and OCR is not perfect. |
| PDF → PPTX/PPT | One slide per page; positioned editable horizontal digital text over preserved artwork | Scanned/rotated typography remains artwork. Charts/tables are not reconstructed as editable PowerPoint objects. PPT uses an additional LibreOffice conversion. |
| PDF → Excel tables | Detected digital tables from all pages, with a worksheet per table | Not a page-layout copy. Scanned-table recognition is not implemented. No detected table results in an error and no published workbook. |
| DOC/DOCX/ODT/RTF ↔ supported Office formats / PDF | Document structure and page layout through LibreOffice | Fonts and unsupported Office-specific features can differ. Macros do not transfer into macro-free destinations. |
| Documents → HTML | Text and embedded image resources in a standalone HTML file | Page layout reflows. |
| Documents → Markdown / LaTeX / RST | Document source plus extracted image assets in a ZIP | These are resource bundles so image references stay usable. Not an exact fixed-page layout. |
| XLS/XLSX/ODS → workbook formats / PDF | Sheets, formulas and formatting through LibreOffice | Legacy XLS has format limits. PDF uses the workbook's printable layout. |
| Workbooks → CSV/TSV/JSON | Every sheet and cached cell values | Multi-sheet text exports are ZIP files. CSV/TSV omit formulas and formatting by design; JSON uses a map of sheet names to typed row arrays. Missing cached formula results produce an error. |
| CSV/TSV → XLSX | All rows/columns and literal text, including leading zeros and `=...` text | CSV has no type schema; values remain text instead of guessing identifiers are numbers/formulas. Native Rust writer uses constant memory. |
| Flat structured records → XLSX | Numbers, booleans and text | Integers exceeding Excel's 15-digit precision stay text. Nested structures are not offered as flat tables. JSON root arrays/nulls aren't offered as TOML. |
| Video → MP4/MOV/WebM/MKV | All non-cover video and audio tracks, supported subtitle tracks, metadata/chapters | MKV also preserves attachments. Bitmap subtitles require MKV. Text subtitles are converted for MP4/MOV/WebM. Compatible codecs are remuxed; others are encoded. Odd dimensions are padded rather than cropped. |
| Audio → MP3/M4A/AAC/Opus/Ogg/FLAC/WAV/AIFF/WMA | Audio; source bit depth for WAV/AIFF and up to 24-bit FLAC; supported metadata | Audio extraction selects the first audio track. Cover art is retained where the destination supports it. Integer FLAC cannot represent arbitrary floating-point PCM losslessly. Lossy formats introduce generation loss. |
| Any regular file → ZIP | The original bytes and filename | This is an archive, not a transcoding operation. |

## Evidence

- Eleven Rust tests cover cancellation, publication, record precision, literal CSV cells, multiple sheets, fullscreen classification, filename timing and quality behavior.
- The optimized release passed 54 baseline conversions (`qa/results.json`) and 51 expanded conversions (`qa/extended-*-results.json`): 16 image/page exports, 15 spreadsheet conversions, 10 document conversions and 10 media conversions. Assertions decode and inspect content, pages, sheets, sample depth and stream counts; they do not merely check file existence.
- Native widget probes exercised the format picker, quality memory and Settings navigation using the app's debug input hook. The [panel capture](assets/screenshots/conversion-panel.png) shows the renderer output with type tiles removed.
- A local old/new 4096 × 3072 image-load sample measured peak process working set at 138.65 MiB before and 117.69 MiB after preview removal. It did not show a CPU speedup. This was one sequential sample, not a general benchmark.

Generated reports and local profiling records are not committed; the [QA guide](qa/README.md) explains the reproducible checks. The tests cover this corpus, not every codec, corrupted file, language or document feature. Unsupported input structures fail explicitly instead of creating plausible-looking partial output.

Implementation references: [Calamine workbook reader](https://docs.rs/calamine/0.37.0/calamine/), [rust_xlsxwriter](https://docs.rs/rust_xlsxwriter/0.101.0/rust_xlsxwriter/), [PyMuPDF page API](https://pymupdf.readthedocs.io/en/latest/page.html), [FFmpeg stream selection](https://ffmpeg.org/ffmpeg.html#Stream-selection). Local runtime pins are in `Cargo.lock`, `engines/requirements.txt`, and `THIRD_PARTY.md`.
