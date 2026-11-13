# roms-curator

[![License: MIT](https://img.shields.io/github/license/hugotigre/roms-curator?style=flat-square)](license)
![CI](https://github.com/hugotigre/roms-curator/actions/workflows/ci.yml/badge.svg)
[![crates.io](https://img.shields.io/crates/v/roms-curator.svg?style=flat-square)](https://crates.io/crates/roms-curator)

---

Desktop app for a merged MAME ROM set. It separates working ROMs from non-working ROMs.

- **roms** holds the working games and the files they need to launch. Point MAME at this folder.
- **rest** holds the non-working and unused files.
- **selected.txt** lists the working games, one name per line, for frontends.

__Index__

- [Using the app](#using-the-app)
- [How this works](#how-this-works)
- [Command line](#command-line)
- [FAQ](#faq)
- [Useful links](#useful-links)
- [Help](#help)

## Using the app

Install [Rust](https://rustup.rs/) and Node.js 26 or newer. From this repository:

```bash
npm install
npm run tauri dev
```

That opens the window. Installers and other ways to build: [build instructions](docs/build-instructions.md).

### Export

1. Stay on **1. Setup**.
2. Enter the MAME set version that matches your ROMs, for example `0.289`.
3. Choose the merged ROM folder and an empty folder for the export. If CHDs live in another folder, add that too.
4. Click **Preview plan**. The first run downloads the MAME machine list and the category file for that version, then shows which games would be kept and which files are missing. Export is the step that copies files.
5. On **2. Plan review**, click **Export** when the plan looks right. **3. Export result** shows what landed in the destination.

The destination must be empty. The app only reads your original collection.

On Setup you can also:

- Turn on **Strict selection** to keep only games MAME rates fully working. The default also keeps games rated imperfect.
- Turn on **Include games with missing ROM or CHD dependencies** to put incomplete games on the launch list. Leave it off and `selected.txt` lists games you can launch.
- Turn on **Re-download listxml and CatVer** to replace the cached metadata for this version.
- Fill **From** and **Through** to export a slice of machine names. Leave both empty for the full list. A kept game still brings along the parent, BIOS, and disk files it needs.

**Cancel** stops a preview or an export. Files already copied stay in the destination. [Export recovery](docs/gui-export-recovery.md) covers partial runs.

From the review screen, **Save Markdown report…** and **Save HTML preview…** write the plan without copying.

## How this works

The export reads the machine list (`listxml`) and categories (`catver.ini`) for your ROM set version, then splits the set. A working machine has driver status and emulation that are both good or imperfect, and every ROM and CHD it needs is in the source. Imperfect sound or graphics does not change that. Strict selection counts a machine as working only when both values are good. Preliminary, mechanical, BIOS, device, and non-arcade categories (computers, consoles, handhelds, and similar) are non-working and stay off the list. Arcade genres such as shooter, puzzle, and slots stay eligible.

The copy writes three things under the destination:

- **roms**: every file a working game needs. That is its merged archive (the parent archive when the game is a clone), BIOS and device archives, and the CHD directories it needs. Point MAME at this folder.
- **rest**: the non-working and unused files. A clone's own zip lands here, because its bytes live in the parent archive. Disk directories no working game needs land here too. On a full export, `roms` and `rest` together hold the whole source, so the original collection can be deleted after a successful run. A **From** / **Through** slice leaves names outside that range in the source unless a working game needs them.
- **selected.txt**: working machine names, one per line, written only after every copy succeeds. By default every required ROM and CHD must be in the source. Frontends use this file. MAME still reads only `roms`.

Missing CHDs show up in the plan and hold a game off `selected.txt` until you add the disk or include incomplete games. When that game's ROM zip is in the source, the copy still takes it, and places it in `rest`.

A merged zip is copied whole, so unselected clones stored inside it stay in the file under `roms`. A game whose zip is in `rest` still needs the parent archive in `roms`.

The app caches listxml and CatVer under `~/.cache/roms-curator/metadata` on Linux and macOS, or `%LOCALAPPDATA%\roms-curator\metadata` on Windows. To supply those files yourself, download the matching MAME build from [mamedev.org](https://www.mamedev.org/release.html) and run `mame.exe -listxml > mame-roms.xml`, and take `catver.ini` from the support files for that ROM set on [progettosnaps.net](https://www.progettosnaps.net/support/). The command line can point at those files directly.

## Command line

The `roms-curator` command runs the same MAME export as the desktop app. Build it with the [build instructions](docs/build-instructions.md), or take `roms-curator-linux` / `roms-curator.exe` from the [GitHub releases](https://github.com/HugoTigre/roms-curator/releases).

```bash
roms-curator --mame-version=0.289 --source-path=/roms --destination-path=/roms-new/
```

Flags, dry runs, and saved plans: [application run instructions](docs/app-run-instructions.md).

Other Rust programs can call the same export code: [Rust API](docs/lib-usage-instructions.md).

## FAQ

[FAQ](docs/faq.md) — the original collection, an existing destination, speed, `selected.txt`, merged sets, and frontends.

## Useful links

- [Download MAME](https://www.mamedev.org/release.html)
- [Online MAME ROM database](http://adb.arcadeitalia.net/lista_mame.php)

## Help

Please feel free to open an [Issue](https://github.com/HugoTigre/roms-curator/issues) if:

- You encounter any problems;
- Something is missing from the documentation material or hard to understand;
- Have any improvement suggestions or feature request.

Project notes: [RFC 0001](docs/rfcs/0001-repair-and-simplify.md), [releasing.md](docs/releasing.md).
