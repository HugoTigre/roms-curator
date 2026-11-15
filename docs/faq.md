# FAQ

### Is there any risk of corrupting the original collection?

No. The tool only copies files and rejects unsafe overlapping source/output/report paths. Do not point the destination inside the source.

### Can I copy into an existing export folder?

No. Existing files or CHD folders in the destination block the run. Use a new folder for a clean export.

### Why is it slow?

Parsing the full MAME XML takes time once per run. Copying large ROM/CHD trees dominates wall clock on slow disks or network mounts.

### Merged vs split sets?

The curator assumes a **merged** collection: clones need parent archives in `roms`. `rest` holds leftover files so you can delete the original tree after a full export. Per-file split selection is not implemented.

### Which MAME version?

Match listxml, CatVer, ROMs, and CHDs to the same set version (e.g. **0.289**). Other versions are untested.

### Why listxml and CatVer?

XML gives emulation status and machine relationships. CatVer gives categories so non-arcade groups (computers, consoles, …) stay off the game list while BIOS/devices still resolve as dependencies.

### Strict emulation?

Default: good **or** imperfect driver and overall emulation. **`--strict-emulation=true`**: both must be **good**.
