# Run instructions

__Index__

- [Get help](#get-help)
- [Mandatory arguments](#mandatory-arguments)
- [Categorize and create a new rom collection](#categorize-and-create-a-new-rom-collection)
- [Simulate operation](#simulate-operation)
- [CHD files](#chd-files)
- [Write a copy plan](#write-a-copy-plan)
- [Include/exclude rom files](#includeexclude-rom-files)
- [Putting everything together](#putting-everything-together)

## Get help

```bash
roms-curator --help
```

## Mandatory arguments

These arguments are mandatory the rest are all optional.

```
--mame-xml-path
--catver-path
--source-path
--destination-path
```

## Categorize and create a new rom collection

```bash
roms-curator \
--mame-xml-path=/mame/mame.xml \
--catver-path=/mame/catver.ini \
--source-path=/roms,/chd-roms \
--destination-path=/roms-new/ \
--report-path=/mame/report/report.md
```

## Simulate operation

Use `--simulation=true` to check the selected operations without copying ROMs or
creating output directories. The source and output trees stay unchanged.

The console shows a Planned summary. Add `--report-path` for a Markdown report
with Planned entries and zero copied ROMs. The report must be outside both trees,
and its parent directory must already exist. Simulation still rejects path/name
conflicts; it does not test write access or available space.

```bash
roms-curator \
--mame-xml-path=/mame/mame.xml \
--catver-path=/mame/catver.ini \
--source-path=/roms,/chd-roms \
--destination-path=/roms-new/ \
--report-path=/mame/report/report.md \
--simulation=true
```

## CHD files

[CHD files](https://fileinfo.com/extension/chd) are usually a separate
collection, and they can take a lot of disk space (around 500Gb).
The destination has two folders. `roms` holds every archive and CHD directory a selected game needs, including a parent or BIOS that is not itself a game. `rest` holds every other source file, including disk directories no selected game needs. `selected.txt` sits beside those folders and lists the selected machine names. It is written only after every copy succeeds. The report must be a different file from `selected.txt`.

A shared disk keeps the ancestor copy in `roms`. A clone disk with its own content also keeps the parent's same-named non-optional disk, because a delta CHD cannot be read without it. When that parent has disks and none is that same-named disk, the parent disk is missing. A parent with no disks is not missing. A missing CHD stays on the list with its ROM archive copied. The readable list of those missing CHDs is a later report.

`--ignore-not-working-chd` is still accepted so older commands parse. It does not move a required disk out of `roms`. Dump quality is reported on `catalog.disks` and does not change the game's category. A merge or delta parent that cannot be resolved is listed as missing media there.

```bash
roms-curator \

--mame-xml-path=/mame/mame.xml \
--catver-path=/mame/catver.ini \
--source-path=/roms,/chd-roms \
--destination-path=/roms-new/
```

## Write a copy plan

`--plan-path` writes a version-2 JSON file before copying when that path is new. It lists each selected game and why it was kept, each file the copy would place in `roms` or `rest`, whether that file is present, its source and destination paths, and its size. Missing files and destination conflicts are in the file. The copy then checks that written plan and stops when a hash, size, or conflict has changed. An existing plan file is read and checked the same way, and is not replaced. A version-1 plan is rejected.

The plan file must end in `.json` and must sit outside the source and destination trees.

```bash
roms-curator \
--mame-xml-path=/mame/mame.xml \
--catver-path=/mame/catver.ini \
--source-path=/roms,/chd-roms \
--destination-path=/roms-new/ \
--plan-path=/mame/plan.json \
--simulation=true
```

## Include/exclude rom files

Because the entire copy of all roms including CHD can take a long time,
it's possible to divide the executions into separate steps, but choosing
which files to include or exclude.

```bash
roms-curator \

--mame-xml-path=/mame/mame.xml \
--catver-path=/mame/catver.ini \
--source-path=/roms,/chd-roms \
--destination-path=/roms-new/ \
--subset-start="a"
--subset-end="de"
```

In this example only names which sort at or after `a` and at or before `de` are
copied, plus parent, BIOS, and device archives and required disk directories for
a selected game inside that range. Other names in the range go to `rest`.
Names outside the range stay in the source.
It's also possible to just set `subset-start` or `subset-end`.

## Putting everything together

```bash
roms-curator \
--mame-xml-path=/mame/mame.xml \
--catver-path=/mame/catver.ini \
--source-path=/roms,/chd-roms \
--destination-path=/roms-new/ \
--report-path=/mame/report/report.md \
--ignore-not-working-chd=true \
--subset-start="0"
--subset-end="zz"
--simulation=true
```
