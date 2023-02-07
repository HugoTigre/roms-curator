## Frequently Asked Questions

---

Q: Is there any risk of corrupting the original MAME ROM collection?

A: No. Categorizing roms or copying them does not apply any changes
to your ROM collection. There is no "move ROMs" operation.

---

Q: Why does this take so long to complete?

A: Categorizing an entire collection of MAME ROMs can take a few minutes
or a few seconds depending on what machine this is run on. Keep in mind
that information needs to first be gathered from `xml` and `txt` files
and they all include the entire rom collection. Copying the roms, however,
can take a lot longer. If we choose to move the entire collection of ROMs
and CHD files, we might be dealing with `6OO+ GB` but at this point, it's
the computer/disc/network performance rate that will affect the time to complete.

---

Q: Does this work with split ROMs Set and merged ROMs Set?

A: Yes. Although a merged ROMs Set is probably recommended if this
is being used to be able to import a clean set to some front-end.

---

Q: Does this work with any MAME version?

A: It should. Obviously not all versions in existence where tested,
but as long as we use the MAME ROM database file (`mame.xml`) and 
MAME support file `catver.ini` for the same version of our ROM Set,
everything should be OK. It's can also be OK to mix ROM and CHD versions,
if for example, we have an older CHD collection and a newer ROM collection,
we just need to be sure that we use the MAME ROM database corresponding to
the more recent version of our Sets and that we don't need the more recent
changes in the CHD Sets. Also if we specify a report path, there is a section
in the report that tells us if any ROM/CHD was not moved, this usually means
that we are missing something in our Sets that is included in the MAME ROM
database. 

---

Q: If `MAME Support File` already contains a list of all working roms
why do we need a `custom logic` to determine working and not working
rom state?

A: This is mainly because those lists are not perfect by any means,
and by cross-checking rom metadata (`mame.xml`) with rom category
(`catver.ini`) and info from the
[Online MAME ROM database](http://adb.arcadeitalia.net/lista_mame.php)
some edge cases and incorrect categorization were detected in the
`MAME Support File`.

---
