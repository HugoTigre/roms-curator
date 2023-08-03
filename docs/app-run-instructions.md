# Run instructions

__Index__

- [Get help](#get-help)
- [Mandatory arguments](#mandatory-arguments)
- [Categorize and create a new rom collection](#categorize-and-create-a-new-rom-collection)
- [Simulate operation](#simulate-operation)
- [Include/exclude useless CHD roms](#includeexclude-useless-chd-roms)
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

To run a simulation (does not create a new rom collection)
you need to set the `simulate` argument and specify the
`report path` file (markdown file). The report file will contain
all operations as if you weren't doing a simulation.

```bash
roms-curator \
--mame-xml-path=/mame/mame.xml \
--catver-path=/mame/catver.ini \
--source-path=/roms,/chd-roms \
--destination-path=/roms-new/ \
--report-path=/mame/report/report.md \
--simulation=true
```

## Include/exclude useless CHD roms

[CHD roms](https://fileinfo.com/extension/chd) are usually a separate
collection, and they can take a lot of disk space (around 500Gb).
For this reason when copying CHD files you can choose not to include
`not working` CHD file, this way your new collection should be smaller.

By default, all CHD files are included, so that you don't miss anything
in your new rom collection, but these files should not be needed if you
only want to create a working collection set.

```bash
roms-curator \

--mame-xml-path=/mame/mame.xml \
--catver-path=/mame/catver.ini \
--source-path=/roms,/chd-roms \
--destination-path=/roms-new/ \
--ignore-not-working-chd=true
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

In this example only roms which ascii name alphabetical order is higher than or
equal to `a` and lower than or equal to `de` will be copied.
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
