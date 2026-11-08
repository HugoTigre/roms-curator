# roms-curator

[![License: MIT](https://img.shields.io/github/license/hugotigre/roms-curator?style=flat-square)](license)
![CI](https://github.com/hugotigre/roms-curator/actions/workflows/ci.yml/badge.svg)
[![crates.io](https://img.shields.io/crates/v/roms-curator.svg?style=flat-square)](https://crates.io/crates/roms-curator)

---

Curates a MAME merged ROM set into a smaller, launch-ready tree plus a game list for frontends.

Implementation status: [RFC 0001](docs/rfcs/0001-repair-and-simplify.md). Backlog: [TODO.md](docs/TODO.md). Releases: [releasing.md](docs/releasing.md).

Possible usages might include:

1. As an Application to copy a merged ROM set into the files selected games need, and keep everything else beside them

1. As a Library if you want to categorize your entire collection
and do with it what you will.

__Index__

- [Requirement / Dependencies](#requirement--dependencies)
- [How this works](#how-this-works)
- [Application run instructions](#application-run-instructions)
- [Build instructions](#build-instructions)
- [Library usage instructions](#library-usage-instructions)
- [FAQ](#faq)
- [Useful links](#useful-links)
- [Help](#help)
- [Desktop curator](#desktop-curator)

## Requirement / Dependencies

This program needs MAME listxml and CatVer for the same ROM set version. You can
point the CLI at files you already have, or pass **`--mame-version`** (for example
`0.289`) and let the tool download and cache them.

Manual download (optional):

- MAME ROM database (xml file)
  - download MAME from [here](https://www.mamedev.org/release.html).
    keep in mind that you need the version that corresponds to your ROM Set version. 
  - then extract the database:
    ```bash
    mame.exe -listxml > mame-roms.xml
    ```
- ROM categories file
  - MAME does not categorize ROMs (some exceptions),
    to categorize ROMs we need a `MAME Support File` file which can
    be downloaded [here](https://www.progettosnaps.net/support/).
  - download the package for your ROMs Set version and extract the
    `catver.ini` file.

Automatic download:

```bash
roms-curator --mame-version=0.289 --source-path=/roms --destination-path=/roms-new/
```

Files are stored under a cache directory. The default root is
`~/.cache/roms-curator/metadata` on Linux and macOS, or
`%LOCALAPPDATA%\\roms-curator\\metadata` on Windows. Set the **full cache root**
(not the XDG base alone) with `--metadata-cache-dir` or `ROMS_CURATOR_CACHE`.
Use `--metadata-refresh=true` to replace cached files.

## How this works

`roms-curator` reads `mame.xml` and `catver.ini`, then categorizes each machine.
A runnable arcade machine is selected when its driver status and emulation are
both good or imperfect. Imperfect sound or graphics does not change that.
Pass **`--strict-emulation=true`** to keep only machines where both values are
**good** (imperfect or preliminary machines drop out). Preliminary, mechanical,
BIOS, device, and non-arcade CatVer groups (computers,
consoles, handhelds, and similar) are not selected. Arcade genres such as
shooter, puzzle, and slots stay eligible. From here it copies your files into
two folders under `destination_dir`.

It can also generate a report with all copied files and/or errors encountered.
`--plan-path` writes a JSON plan of the copy first: which games are kept and why, which files are present or missing, how large they are, and any destination conflicts. The copy checks that plan against the disk before it writes.

- **roms**: every file a selected game needs. That is its merged archive (the parent archive when the game is a clone), BIOS and device archives, and the CHD directories it needs. Point MAME at this folder.
- **rest**: every other source file. A clone's own zip lands here, because its bytes live in the parent archive. Disk directories no selected game needs land here too. This folder is what makes it safe to delete the original collection.
- **selected.txt**: launch-ready machine names (by default every required ROM and CHD must be in the source), one per line, written only after every copy succeeds. Use `--include-incomplete-games=true` to list games with missing dependencies too. Frontends use this file; MAME still reads only `roms`. The report path must not be `selected.txt`.

Required CHDs go to `roms` when present for **launch-ready** games. Missing CHDs are listed under **Missing CHDs** and **hold back** a game from `selected.txt` until you add the disk or pass `--include-incomplete-games=true`. If the game’s ROM zip is in the source, it is still copied, but to **`rest`** (not `roms`), because held-back games are outside the launch export set.

A merged zip is copied whole, so unselected clones stored in it stay inside the file in `roms`. MAME uses only the `roms` folder. `rest` is the remainder, and a game left there will not run on its own when its parent archive moved into `roms`.

## Application run instructions

See [here](docs/app-run-instructions.md)

## Build instructions

See [here](docs/build-instructions.md)

## Library usage instructions

See [here](docs/lib-usage-instructions.md)

## FAQ

See [here](docs/faq.md)

## Useful links

- [Download MAME](https://www.mamedev.org/release.html)
- [Online MAME ROM database](http://adb.arcadeitalia.net/lista_mame.php)

## Help

Please feel free to open an [Issue](https://github.com/HugoTigre/roms-curator/issues) if:

- You encounter any problems;
- Something is missing from the documentation material or hard to understand;
- Have any improvement suggestions or feature request.

## Desktop curator

Build and run the Tauri app: [build instructions](docs/build-instructions.md). Export cancellation and partial runs: [gui-export-recovery.md](docs/gui-export-recovery.md).
