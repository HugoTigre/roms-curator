# roms-curator

Helper utility to manage sets of roms.
Currently only works for mame roms.

You can use this if you want to create a new rom set
with only working roms from your current rom set.

- [roms-curator](#roms-curator)
  - [Get needed rom metadata files](#get-needed-rom-metadata-files)
  - [How this works](#how-this-works)
  - [Run instructions](#run-instructions)
  - [QA](#qa)
  - [Useful links](#useful-links)
  - [Planned for the future (maybe)](#planned-for-the-future-maybe)

## Get needed rom metadata files

This program has 2 mandatory dependencies:

- MAME rom database (xml file)
  - download MAME from [here](https://www.mamedev.org/release.html).
    keep in mind that you need the version that corresponds to your rom set version. 
  - then extract the database:
    ```bash
    mame.exe -listxml > mame-roms.xml
    ```
- Rom categories file
  - MAME does not categorize roms (some exceptions),
    to categorize roms you need the a `MAME Support File` file which can
    be downloaded [here](https://www.progettosnaps.net/support/).
  - download the package for your roms set version and extract the
    catver.ini file.

## How this works

`roms-curator` will use `mame.xml`, `catver.ini` and some custom logic 
(from trial and error) to go through your rom collection and categorize all
roms with `working` or `not-working` states as well as specific sub-categories,
like type of rom/game (bios, system, mechanical, etc). From here it will copy
your roms to a subdirectory in the specified `destination_dir`.

It can also generate a report with all copied files and/or errors encountered.

Sub-directories include: 
- working: for all roms in working/playable state.
- other: for all roms NOT in working/playable state. 
This also includes bios, system, casino, mechanical and some other
non-playable roms.
- chd_working: for all CHD roms in working/playable state.
- chd_other: for all CHD roms NOT in working/playable state.

Having the roms categorized in these sub-folders will allow you to
only import working roms (working folder) into your mame front-end,
while still giving MAME executable access to all roms by adding all
directories as roms directories (this is needed because of rom dependency,
like a working rom needing a bios rom or a CHD rom to work properly).

## Run instructions

See [here](docs/run-instructions.md)

## FAQ

See [here](docs/faq.md)

## Useful links

- [Download MAME](https://www.mamedev.org/release.html)
- [Online MAME rom database](http://adb.arcadeitalia.net/lista_mame.php)

## Planned for the future (maybe)

- Add option to automatically download dependencies (mame xml and catver.ini).
