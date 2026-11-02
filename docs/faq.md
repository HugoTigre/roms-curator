## Frequently Asked Questions

### Is there any risk of corrupting the original MAME ROM collection?

No. The tool copies your ROMs and rejects unsafe source/output paths. Keep those folders unchanged while it runs.

### Why does this take so long?

Reading the full MAME database takes time. Copying large ROM and CHD collections takes longer and depends on your disk and network speed.

### Does this work with split and merged sets?

Yes, it can organize either format. Keep all generated folders available to MAME, since games can depend on files in other folders. The working folder alone may be incomplete.

### Does this work with any MAME version?

Use metadata matching your ROM collection and MAME version. Games that require CHDs also need matching CHD files. Compatibility with every MAME version has not been verified.

### Why use both mame.xml and catver.ini?

MAME's XML describes emulation status and machine relationships. CatVer supplies game categories. The tool combines them to classify the collection.
