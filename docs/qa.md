## Questions and Answers

- Q: If `MAME Support File` already contains a list of all working roms
why do we need a `custom logic` to determine working and not working
rom state?
  - A: This is mainly because those lists are not perfect by any means,
  and by cross-checking rom metadata (`mame.xml`) with rom category
  (`catver.ini`) and info from the
  [Online MAME rom database](http://adb.arcadeitalia.net/lista_mame.php)
  some edge cases and incorrect categorization was detected in `MAME Support File`.
