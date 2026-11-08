# CLI usage

```bash
roms-curator --help
```

## Required flags

- `--source-path` — one or more folders (ROM set; add a second path for CHDs if separate).
- `--destination-path` — empty folder root for `roms`, `rest`, and `selected.txt`.
- Metadata: **`--mame-version=0.289`** (download + cache), or **`--mame-xml-path`** and **`--catver-path`**.

Optional metadata: `--metadata-cache-dir`, `--metadata-refresh=true`.

## Typical export

```bash
roms-curator \
  --mame-version=0.289 \
  --source-path=/roms,/chd-roms \
  --destination-path=/export/ \
  --report-path=/export/report.md
```

- **`roms`** — archives and CHD directories MAME needs for launch-ready games. Point the emulator here.
- **`rest`** — every other source entry this run copies (e.g. clone zips whose bytes live in a parent archive).
- **`selected.txt`** — launch-ready machine names (see below). Written only after every planned copy succeeds.

Default policy keeps arcade games when driver status and emulation are each good or imperfect. Use **`--strict-emulation=true`** for good-only.

## Launch-ready list vs held back

By default, a game needs its required ROM archives and CHDs in the source to appear in **`selected.txt`**. Held-back games still appear in the plan/report; if their ROM zip is in the source it is copied to **`rest`**, not `roms`, because export only retains launch-ready dependencies under `roms`. Pass **`--include-incomplete-games=true`** to list incomplete games on `selected.txt` and copy their required media into `roms`.

## Simulation and plan

**Dry run** (no destination folders, no copies):

```bash
roms-curator ... --simulation=true --report-path=/tmp/report.md
```

**JSON plan** (outside source and destination trees):

```bash
roms-curator ... --plan-path=/tmp/plan.json
```

New plans are written once; copy rechecks an existing plan. Version-1 plans are rejected.

## Subset by name

```bash
roms-curator ... --subset-start=a --subset-end=de
```

Only games whose names fall in the range are considered for the list; required parents, BIOS, devices, and CHD dirs for those games still go to `roms`. Names outside the range that are not required stay in the source.

## CHD notes

Shared and delta CHD rules are handled when building the plan. **`--ignore-not-working-chd`** is accepted for compatibility and does not remove required disks from `roms`. See [FAQ](faq.md) and the README **How this works** section.

## Desktop app

Same rules as the CLI. Partial exports and cancel: [gui-export-recovery.md](gui-export-recovery.md). Build: [build-instructions.md](build-instructions.md).
