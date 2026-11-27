## DBviewy 0.22.1-dbviewy.1

DBviewy is based on Octa 0.22.1. This preview includes the source and a
download for Apple Silicon Macs. The Mac app is ad hoc signed, not
notarised. There are no Windows, Linux or Intel Mac binaries in this
release. The Octa downloads are a separate product. On a Mac, the update dialog
links to DBviewy's own downloads.

### What changed

- The zoom controls scale the table without enlarging the menu bar,
  ribbon or tabs. DBviewy remembers the zoom level. Use the controls
  at the bottom right or reset the table to 100% with Command-0.
- A line chart can show a horizontal target without a helper column.
  Monthly labels in exported line and scatter charts line up with
  the points. The chart can be exported as SVG, PNG or PDF.
- Split a column with a preview before changing data. Open a value
  frequency report in a separate table instead of altering the source.
- Spreadsheet formulas cover error calculations such as SUMXMY2,
  SUMSQ and SQRT. For supported cell edits, the XLSX writer preserves
  other workbook parts. Use Save As for edits outside that safe path.

### Check the download

Download the arm64 ZIP and its SHA256SUMS file into the same folder.
Run `shasum -a 256 -c SHA256SUMS` before opening the archive. Open a copy
of your workbook when trying a new build. If macOS blocks the app,
see the [macOS install instructions](https://github.com/Supersynergy/DBviewy/blob/master/pkg/README-macos.md).
