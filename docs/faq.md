## Frequently Asked Questions

### Is there any risk of corrupting the original MAME ROM collection?

No. The tool copies your ROMs and rejects unsafe source/output paths. Keep those folders unchanged while it runs.

### Can I copy into an existing collection?

Existing selected files or CHD folders stop the run before copying. Use a fresh output folder; the tool does not overwrite files or merge CHD folders.

### Why does this take so long?

Reading the full MAME database takes time. Copying large ROM and CHD collections takes longer and depends on your disk and network speed.

### Does simulation write anything?

Only a report if you request one. It leaves your source and output folders unchanged and labels operations Planned.

### Does this work with split and merged sets?

The export assumes a merged collection: a clone requires its parent archive, plus BIOS and device archives, even when those machines are not games. Those files go in `roms` with the CHD directories the selected games need. `rest` keeps every other source file. MAME uses `roms`. A name-range subset still copies required archives and disk directories when their names fall outside the range, and leaves other out-of-range names in the source. Split-set file selection is a later decision.

### Does this work with any MAME version?

Use metadata matching your ROM collection and MAME version. Games that require CHDs also need matching CHD files. Compatibility with every MAME version has not been verified.

### Why use both mame.xml and catver.ini?

MAME's XML describes emulation status and machine relationships. CatVer supplies game categories. The visible list is good or imperfect arcade games. A clone stays on that list when its parent does not, and the parent's archive stays required. BIOS, devices, and other support machines stay in the catalog outside that list. CatVer groups such as computers and consoles are classified as systems, not dropped as genres.
