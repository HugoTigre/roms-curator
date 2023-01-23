# Run instructions

- [Run instructions](#run-instructions)
  - [Get help](#get-help)
  - [Categorize and create a new rom collection](#categorize-and-create-a-new-rom-collection)
  - [Simulate operation](#simulate-operation)
  - [Include/exclude useless CHD roms](#includeexclude-useless-chd-roms)
  - [Putting everything together](#putting-everything-together)

## Get help

```bash
roms-curator --help
```

## Categorize and create a new rom collection

Setting the `report path` file (markdown file) is optional.
The rest of the arguments are mandatory.

```bash
roms-curator \
--mame_xml_path=/mame/mame.xml \
--catver_path=/mame/catver.ini \
--source_path=/roms,/chd-roms \
--destination_path=/roms-new/ \
--report_path=/mame/report/report.md
```

## Simulate operation

To run a simulation (does not create a new rom collection)
you need to set the `simulate` argument and specify the
`report path` file (markdown file). The report file will contain
all operations as if you weren't doing a simulation.

```bash
roms-curator \
--mame_xml_path=/mame/mame.xml \
--catver_path=/mame/catver.ini \
--source_path=/roms,/chd-roms \
--destination_path=/roms-new/ \
--report_path=/mame/report/report.md \
--simulate=true
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
--mame_xml_path=/mame/mame.xml \
--catver_path=/mame/catver.ini \
--source_path=/roms,/chd-roms \
--destination_path=/roms-new/ \
--ignore_not_working_chd=true
```

## Putting everything together

```bash
roms-curator \
--mame_xml_path=/mame/mame.xml \
--catver_path=/mame/catver.ini \
--source_path=/roms,/chd-roms \
--destination_path=/roms-new/ \
--report_path=/mame/report/report.md \
--ignore_not_working_chd=true \
--simulate=true
```
