# Security

## Supported version

Security fixes target the current `main` branch. Dropflip is a source preview; there is no automatic updater. Rebuild the application and its PDF/image helper after updating their sources or dependencies.

## Report a vulnerability

Use [GitHub's private vulnerability reporting](https://github.com/AlthafTM/dropflip/security/advisories/new). Include the affected commit, conversion route, reproduction steps and expected impact. Use a small synthetic file without personal data or credentials. Please avoid public issues containing exploit files or private information.

## Boundaries and limits

- Files are processed locally under the signed-in user's permissions. External engines are separate processes, not a complete operating-system sandbox. Keep those tools updated and install them from trusted sources.
- Pandoc readers and writers run with `--sandbox` and fail on warnings. Resources packaged inside documents can be preserved. External file/URL resources are not automatically authorized; a conversion that needs them can fail. The Typst PDF step has a temporary project root separate from the source folder.
- PDF/SVG raster exports and full-page OCR reject pages over 32 million pixels or 16,384 pixels on either edge before rendering. These limits do not bound every allocation inside third-party parsers.
- Outputs are staged and published without overwriting an existing file. Failed conversions do not publish their staging output.
- Converted HTML and documents can retain active content or links. Conversion does not sanitize a document or establish that it is safe to open.
- `DROPFORM_TOOLS`, `PATH`, configuration overrides and the installed tools directory are trusted operator configuration. Do not point them at an untrusted directory or run Dropflip as administrator.

Dependency alerts, CodeQL and the dependency-audit workflow supplement source review. A passing check does not establish a numerical security score or guarantee the absence of vulnerabilities.
