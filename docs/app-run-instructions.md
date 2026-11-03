# Run instructions

__Index__

- [Get help](#get-help)
- [Mandatory arguments](#mandatory-arguments)
- [Categorize and create a new rom collection](#categorize-and-create-a-new-rom-collection)
- [Simulate operation](#simulate-operation)
- [CHD files](#chd-files)
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
Copying keeps a disk directory only when a selected game needs that disk.
A shared disk keeps the ancestor copy. A clone disk with its own content
also keeps the parent's same-named non-optional disk, because a delta CHD
cannot be read without it. When that parent has disks and none is that
same-named disk, the parent disk is missing. A parent with no disks is not missing.
A disk directory that no selected game needs is left out.

`--ignore-not-working-chd` is still accepted so older commands parse. It no
longer drops a disk a selected game needs, and it does not copy unused disks
either. Dump quality is reported on `catalog.disks` and does not change the
game's working category. A merge or delta parent that cannot be resolved is
listed as missing media there.

```bash
roms-curator \

--mame-xml-path=/mame/mame.xml \
--catver-path=/mame/catver.ini \
--source-path=/roms,/chd-roms \
--destination-path=/roms-new/
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
a selected game inside that range.
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
