# GUI export recovery

The desktop app and CLI share the same export rules (path safety, `roms`/`rest` split, launch-ready `selected.txt`).

## Stopped or failed export

- **Source collection** is never modified.
- **Each file that finished copying** stays in `roms` or `rest`. The tool does not roll back completed copies when a later file fails or when you cancel.
- **`selected.txt`** is written only after every planned copy succeeds. A cancelled run, a copy error, or a failure to publish that list leaves it absent or unchanged.
- **Staging files** from an interrupted copy (`.roms-curator-*` siblings next to a destination file) may remain after a crash or force-quit. Normal failures and GUI cancellation clean staging up; abrupt termination does not guarantee that.

## Cancelling preview or export

Use **Cancel** while a plan or export is running. The app sets a cooperative stop flag. Downloading listxml and CatVer runs to completion; the stop is noticed at the next catalog-load check. After that, the core checks the flag while reading CatVer, parsing the MAME XML, building the plan (including after metadata hashes), and before each file copy, then once more after the copies and before `selected.txt`. Already-copied files remain. Preview cancellation does not write anything under the destination. A cancelled export clears the plan preview so you preview again before another export into the same folder.

## Finishing later

There is no resume-from-checkpoint mode. To complete a partial export:

1. Fix missing source files or destination conflicts shown in the plan or report.
2. Run **Preview plan** again, then **Export**, or use the CLI with `--plan-path` when you saved a plan file.

Use a **fresh destination folder** when you want a clean tree; the tool refuses to overwrite existing output files or merge CHD folders.

## Reports

If you set a report path, a run that ends with errors still may write a report listing copied, failed, and missing items. Cancellation returns an error to the GUI and does not show an export summary panel.
