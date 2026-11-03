# roms-curator

[![License: MIT](https://img.shields.io/github/license/hugotigre/roms-curator?style=flat-square)](license)
![CI](https://github.com/hugotigre/roms-curator/actions/workflows/ci.yml/badge.svg)
[![crates.io](https://img.shields.io/crates/v/roms-curator.svg?style=flat-square)](https://crates.io/crates/roms-curator)

---

Helper utility to manage Sets of ROMs.
Currently only works for MAME ROMs,
for example to create a working only ROM Set.

Development planning: [repair and simplification RFC](docs/rfcs/0001-repair-and-simplify.md).
Release rules: [CI and release delivery](docs/releasing.md).

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
- [Planned for the future (maybe)](#planned-for-the-future-maybe)

## Requirement / Dependencies

This program has 2 external dependencies that are not included here,
but are easy to get:

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

## How this works

`roms-curator` reads `mame.xml` and `catver.ini`, then categorizes each machine.
A runnable arcade machine is selected when its driver status and emulation are
both good or imperfect. Imperfect sound or graphics does not change that.
Preliminary, mechanical, BIOS, device, and non-arcade CatVer groups (computers,
consoles, handhelds, and similar) are not selected. Arcade genres such as
shooter, puzzle, and slots stay eligible. From here it copies your files into
two folders under `destination_dir`.

It can also generate a report with all copied files and/or errors encountered.
`--plan-path` writes a JSON plan of the copy first: which games are kept and why, which files are present or missing, how large they are, and any destination conflicts. The copy checks that plan against the disk before it writes.

- **roms**: every file a selected game needs. That is its merged archive (the parent archive when the game is a clone), BIOS and device archives, and the CHD directories it needs. Point MAME at this folder.
- **rest**: every other source file. A clone's own zip lands here, because its bytes live in the parent archive. Disk directories no selected game needs land here too. This folder is what makes it safe to delete the original collection.
- **selected.txt**: the exact selected machine names, one per line, written after every file in the run has been copied. A failed copy does not publish this list. A frontend uses it. `mayday` can be in `roms` because `batlzone` is inside that zip, and still stay off the list. The report file must be a different path.

A selected game keeps every required disk. That includes one shared ancestor CHD, and the parent's same-named non-optional CHD when a clone disk has its own content and may be stored as a delta. A bad or missing dump does not remove the game. A CHD that is not in the source is recorded as missing and is not copied; the game and its ROM archive stay.

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

## Planned for the future (maybe)

- Add option to automatically download dependencies (mame xml and catver.ini).
- Verify ROM Set integrity.
