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

1. As an Application to create a new ROM Set
separating working and non-working ROMs (including CHDs)

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
A runnable arcade machine is working when its driver status and emulation are
both good or imperfect. Imperfect sound or graphics does not change that.
Preliminary, mechanical, BIOS, device, and non-arcade CatVer groups (computers,
consoles, handhelds, and similar) are not working. Arcade genres such as
shooter, puzzle, and slots stay eligible. From here it copies your ROMs to a
subdirectory in the specified `destination_dir`.

It can also generate a report with all copied files and/or errors encountered.
`--plan-path` writes a JSON plan of the copy first: which games are kept and why, which files are present or missing, how large they are, and any destination conflicts. The copy checks that plan against the disk before it writes.

Sub-directories include: 
- **working**: for all ROM files in working/playable state (excluding ROMs with CHD dependencies).
- **other**: for all ROM files NOT in working/playable state (excluding ROMs with CHD dependencies). 
This also includes BIOS, system, mechanical, and preliminary machines.
- **chd_working**: CHD directories required by a working game, and ROM archives of working machines that list disks.
- **chd_other**: CHD directories required by a selected game when the machine that stores the disk is not working, and ROM archives of other machines that list disks.

A selected game keeps every required disk. That includes one shared ancestor CHD, and the parent's same-named non-optional CHD when a clone disk has its own content and may be stored as a delta. Disk directories that no selected game needs are not copied. A bad or missing dump does not by itself move the game out of working.

Having the ROMs categorized in these sub-folders will allow you to
only import working ROMs (working folder) into your mame front-end,
while still giving MAME executable access to all ROMs by adding all
directories as ROMs directories (this is needed because of ROM dependency,
like a working ROM needing a bios ROM or a CHD ROM to work properly).

Do not delete `chd_other` just because those machines are not games. A working clone can still need a parent CHD stored there.

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
