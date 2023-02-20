# roms-curator

[![License: MIT](https://img.shields.io/github/license/hugotigre/roms-curator?style=flat-square)](license)
![CI](https://github.com/hugotigre/roms-curator/actions/workflows/build.yml/badge.svg)
[![crates.io](https://img.shields.io/crates/v/roms-curator.svg?style=flat-square)](https://crates.io/crates/roms-curator)

---

Helper utility to manage Sets of ROMs.
Currently only works for MAME ROMs,
for example to create a working only ROM Set.

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

`roms-curator` will use `mame.xml`, `catver.ini` and some custom logic 
(from trial and error) to go through your ROM collection and categorize all
ROMs with `working` or `not-working` states as well as specific sub-categories,
like type of ROM/game (bios, system, mechanical, etc). From here it will copy
your ROMs to a subdirectory in the specified `destination_dir`.

It can also generate a report with all copied files and/or errors encountered.

Sub-directories include: 
- **working**: for all ROM files in working/playable state (excluding ROMs with CHD dependencies).
- **other**: for all ROM files NOT in working/playable state (excluding ROMs with CHD dependencies). 
This also includes bios, system, casino, mechanical and some other non-playable ROMs.
- **chd_working**: for all CHDs and ROMs dependent on CHD files in working/playable state.
- **chd_other**: for all CHDs and ROMs dependent on CHD files NOT in working/playable state.

Having the ROMs categorized in these sub-folders will allow you to
only import working ROMs (working folder) into your mame front-end,
while still giving MAME executable access to all ROMs by adding all
directories as ROMs directories (this is needed because of ROM dependency,
like a working ROM needing a bios ROM or a CHD ROM to work properly).

You might also want to reduce the size of your collection and one way to
achieve that without breaking anything is to delete the non-working CHD directories.
Complete Set of CHD files can be `500+ GB`.

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

## Planned for the future (maybe)

- Add option to automatically download dependencies (mame xml and catver.ini).
- Verify ROM Set integrity.
