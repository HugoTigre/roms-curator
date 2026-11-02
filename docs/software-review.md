# roms-curator software review

Reviewed 2 November 2026. Source revision: `3786b54`, application version `0.1.5`.

Implementation tracking now lives in [RFC 0001](rfcs/0001-repair-and-simplify.md).
This review records the original inspection; the RFC and [release guide](releasing.md)
record subsequent CI repairs and validation. Formatting changes can shift source
line numbers from the reviewed revision.

The core idea makes sense: select games using MAME metadata, use CatVer for categories, and preserve every file needed to launch the selection. The current implementation is useful as a four-folder organizer, but I would repair its file handling and selection model before using it to create a smaller collection.

The main difficulties are MAME’s storage and dependency rules, plus what “working” should mean. Rust is suitable for this tool; a language rewrite is unnecessary.

## Confirmed target and recommendation for this collection

The user has clarified the intended workflow:

- Source: a merged MAME 0.289 collection, with corresponding CHDs assumed available separately.
- Selection: playable arcade games, accepting minor emulation issues.
- Result: a physically smaller collection that can run independently of the complete source, plus a clean frontend list. Let the frontend group duplicate versions.
- Targets: Windows and Android, both using MAME 0.289 as confirmed by the user. LaunchBox is the stated frontend. Device-specific performance is an optional later concern.

“A smaller collection” and “a clean frontend list” are related but separate outcomes. Hiding a machine in a frontend changes the visible list without reclaiming disk space. Removing storage can reclaim space, but only when no retained game needs it. A BIOS archive can remain essential without being a visible game. In a merged archive, an excluded machine and a selected clone may share one physical file.

For this use case, **keep the existing merged source and initially export the required merged archives, BIOS/device archives, and CHDs into one conventional ROM directory**. Generate an explicit list of selected machine names alongside it. Retain an archive whenever any selected machine needs its contents, regardless of the archive owner's own playability. This can remove wholly unneeded families and disks while retaining unwanted variants inside required merged archives. The exact savings require a source inventory; no size estimate has been measured.

There is no need to obtain a split collection first. Split output becomes useful if individual clone archives are required by an importer or if selectively rebuilding archives materially reduces the desired subset. It still has parent dependencies. For a complete collection, merged storage saves space relative to split; for a particular curated subset, the result depends on the selected machines and whether archives are rebuilt. Rebuild from the existing source with a suitable ROM manager if needed, rather than assuming a fresh download is required. See [MAME's storage definitions](https://docs.mamedev.org/usingmame/aboutromsets.html).

Use the corresponding **0.289 XML and category data** for selection. The 0.244 measurements later in this report diagnose the existing implementation; they are not counts or savings for the user's 0.289 collection. Accept good/imperfect overall driver status by default, then validate media. Genre exclusions should be configurable preferences. Do not silently remove playable genres merely to make the list smaller.

LaunchBox already provides a [MAME full-set importer](https://feedback.launchbox-app.com/en/help/articles/4867028-mame-and-arcade-sets-full-set-basics-for-beginners) that filters library entries and groups clones without removing ROM files. This covers much of the display goal. For a reduced merged export, test import and launching against the explicit selected-machine list: a scan of archive filenames cannot discover all selected clones. Full-set import assumes a complete or nearly complete collection, so do not assume it accurately represents an arbitrary reduced export. Frontend integration is a separate acceptance test.

The user has fixed MAME 0.289 as the emulator target on both Windows and Android. Use the same versioned media requirements for both exports. Hardware speed and available controls can be optional later preferences, kept separate from emulation status; no Android performance exclusion should be inferred from XML alone. No further emulator-selection detail is required for this review or the core export design.

The first concrete validation should use 0.289 metadata plus a small sample containing a parent, a clone stored in a merged archive, a BIOS/device-dependent game, and a CHD game. Verify the export with the complete source absent from MAME's search paths, then check LaunchBox's displayed names and launch targets and test the intended Android emulator.

## What the existing program actually does

1. Loads `catver.ini` into a name-to-category map, stopping at `[VerAdded]`.
2. Loads the entire `mame -listxml` document and builds one record per machine.
3. Assigns BIOS and system categories first. Other machines are rejected if mechanical, marked `driver.status=preliminary`, named in a hardcoded exclusion list, or carrying both sound and graphics feature entries. Remaining machines with good/imperfect emulation become working.
4. Applies a second dependency/disk-status demotion pass.
5. Scans immediate children of each source folder. A matching filename stem determines the machine. It copies the complete file or directory into `working`, `other`, `chd_working`, or `chd_other`.
6. Optionally writes a Markdown copy report.

By default, it copies both working and other machines. There is no CLI option to copy only selected working games with their dependencies. The CHD exclusion switch only omits machines that have disks and are classified outside Working.

This matches the README’s intended frontend workflow: import games from the working folder, but configure MAME to search all four folders. That can retain dependencies for a complete source. Keeping only `working`, or deleting `chd_other`, has a different and weaker guarantee.

The library can filter its map before copying, but the copier does not restore dependencies removed by that filter. See [selection and copying](/Users/tigre/repos/roms-curator/src/core/roms_service.rs:168).

## MAME assumptions and the merged-set question

A machine is a launchable target; an archive is a storage container. Their names often match, but they are not interchangeable concepts.

- **Merged:** a parent archive can also store its clones. Classifying the archive solely by the parent can hide a playable clone or retain unwanted variants. Whole-archive copying cannot make its contents working-only.
- **Split:** a clone can require its parent archive. Filtering filenames without resolving dependencies can break it.
- **Non-merged/standalone:** these are easier to select individually, but verify the producer’s convention concerning BIOS/device contents. Do not infer that every archive includes them merely from a collection label.
- **CHDs:** selected games can share disks with parents; delta CHDs require their parent CHDs. Those relationships also need preservation.

These storage relationships are documented by [MAME](https://docs.mamedev.org/usingmame/aboutromsets.html) and its [media search documentation](https://docs.mamedev.org/usingmame/assetsearch.html). The current clrmame documentation distinguishes [nonmerged from standalone](https://mamedev.emulab.it/clrmamepro/binaries/readme.html), illustrating why collection terminology needs checking.

**Answer:** this program does not require merged input, and it does not detect a set mode. It can copy merged, split, non-merged, ZIP, 7z, and directory entries by name. Its claim to support merged/split is reasonable for broad organization with all folders retained, but does not establish a dependency-complete curated export for those formats.

There are real examples in the bundled 0.244 metadata: the independent model finds 35 selected clones whose parents are categorized NotWorking, including `batlzone` → `mayday`. With merged storage, a scan sees the parent archive and its classification, rather than a separate archive for each selected clone. The 35 is a metadata observation, not a measured failure count for an actual collection.

## Findings and fixes, in priority order

### 1. High — overlapping paths and overwrites can damage files

The CLI compares source and destination as case-insensitive strings. It does not establish filesystem identity, check ancestry, or protect against symlink aliases. The copy operation overwrites existing destination files, and duplicate names from later source folders overwrite earlier copies.

For example, source `/collection/working` and destination `/collection` pass the equality check. A working archive’s computed destination is its own source path. Rust’s [file-copy documentation](https://doc.rust-lang.org/std/fs/fn.copy.html) explicitly warns that copying a file onto itself can truncate it. A destination symlink can also point back into the source. These are source-level risks; I did not execute destructive reproductions.

**Fix:** resolve existing paths and the existing ancestors of new paths, reject source/output overlap, define a symlink policy, and check each planned source/target pair for aliasing. Reject conflicting destinations by default. Copy to a temporary sibling file, check the result, then publish it atomically with explicit overwrite behavior. Check report/output paths too.

Evidence: [path validation](/Users/tigre/repos/roms-curator/src/core/args.rs:168), [file copying](/Users/tigre/repos/roms-curator/src/core/roms_service.rs:286), [recursive copying](/Users/tigre/repos/roms-curator/src/utils.rs:46). The FAQ’s unconditional source-safety claim needs qualification until this is fixed.

### 2. High — selection does not resolve storage dependencies

The parsed model retains `cloneof` and `romof`, but discards device references, ROM requirements/hashes, disk hashes/merge information, and optionality. The copy phase only visits entries already on disk. It never turns a selected machine into the complete set of storage objects required to run it.

Consequences include missing BIOS/device archives after filtering, split clones without parents, and playable clones hidden inside an excluded merged parent. Alphabetical subset filtering can exclude needed dependencies too.

**Fix:** keep selected launch targets separate from required storage. Resolve parent, BIOS, device, and disk requirements under an explicit source-set mode. Retain dependencies even when they are excluded from the game list. Start with whole-archive copying; delegate archive rebuilding to an established ROM manager if smaller archive contents are needed.

Evidence: [metadata extraction](/Users/tigre/repos/roms-curator/src/core/roms_service.rs:17), [source-driven copy loop](/Users/tigre/repos/roms-curator/src/core/roms_service.rs:189).

### 3. High — skipping “non-working CHDs” can remove required data

`ignore_not_working_chd` checks the classification of the machine owning the source entry. It does not check whether another selected machine uses that disk. The same decision skips both a disk directory and a ROM archive for that machine. Whole-directory copying can also retain unrelated disks.

**Fix:** first select games, then retain the union of their required disks, including shared and parent CHDs. If the user excludes all CHD games, remove those launch targets explicitly. If they exclude unselected disks, retain every disk required by a selected target. Deprecate the current switch’s blanket guarantee.

Evidence: [CHD copy filter](/Users/tigre/repos/roms-curator/src/core/roms_service.rs:274); [MAME CHD lookup](https://docs.mamedev.org/usingmame/assetsearch.html#chd-format-disk-images).

### 4. High — success does not mean a complete or runnable result

`all_ok` means no attempted copy returned false. A missing archive or disk is not attempted and therefore does not count as a failure. Empty source folders can report success. An empty/corrupt archive with the right filename is copied without inspection.

Individual copy failures are accumulated in `Report`, but the CLI does not inspect `all_ok`; it can exit successfully after failed copies. Report-generation errors explicitly exit with status 0. Simulation marks planned files as successfully moved.

**Fix:** distinguish selection, availability, copy outcome, and verification. Record missing required files and concrete I/O errors; return nonzero for incomplete execution or a failed requested report. Label simulated actions Planned. Provide optional validation with the matching MAME executable and [its `-verifyroms` command](https://docs.mamedev.org/commandline/commandline-all.html). Run verification for selected machine names against output paths only, so the original collection cannot mask missing output. Verification checks media; it does not establish subjective playability or acceptable performance.

For a separate CHD integrity pass, optionally use [`chdman verify`](https://docs.mamedev.org/tools/chdman.html#verify) without its repair option. Treat availability checks and complete disk-content integrity as distinct verification levels.

Evidence: [success calculation](/Users/tigre/repos/roms-curator/src/core/roms_service.rs:235), [CLI outcome handling](/Users/tigre/repos/roms-curator/src/main.rs:26).

### 5. Medium — the dependency demotion pass contains concrete logic errors

The recursive call in `should_demote_rom` discards its result, so a bad transitive dependency does not propagate. Missing references return false. There is no cycle protection. Separately, an `else if` means a machine with a qualifying `romof` relationship skips its own bad/nodump disk checks.

**Fix:** replace this pass with explicit dependency resolution and separate media diagnostics. If retaining it temporarily, propagate recursive results, detect missing references and cycles, and make disk checks independent.

Do not automatically exclude a clone because its parent’s emulation is bad: the shared bytes may still be exactly what the clone needs. Likewise, “excluded from the game list” must not imply “unusable as a dependency.” A one-line recursion fix alone would preserve that conceptual problem.

Evidence: [dependency pass](/Users/tigre/repos/roms-curator/src/core/roms_service.rs:317), [discarded recursive result](/Users/tigre/repos/roms-curator/src/core/roms_service.rs:347).

### 6. Medium — “working” mixes emulation facts with personal curation choices

The sound/graphics rule counts feature types and ignores `Feature.status`. It rejects a machine with both features even if both are merely imperfect. It does not express a consistent severity policy for other feature types. When XML contains both `status` and `overall`, parsing chooses `status`, potentially losing the aggregate severity.

MAME already computes an overall frontend status. Its [XML exporter](https://github.com/mamedev/mame/blob/master/src/frontend/mame/infoxml.cpp) describes good as fully emulated, imperfect as having minor issues, and preliminary as having major issues or not working. It also emits aggregate feature information. Use that existing summary as the default; make stricter feature rules explicit preferences.

On the bundled full 0.244 metadata, the independent model identifies **749 machines rejected by the extra sound/graphics count despite otherwise acceptable driver/emulation fields**, including `1on1gov`, `astrass`, and `bakubaku`. This measures policy impact, not proof that those games meet the user’s playability preferences.

**Fix:** separate emulation quality, game type, user inclusion, and media availability. Offer a playable preset accepting good/imperfect overall status and a strict preset accepting only good. Keep overrides in a small versioned config with reasons. CatVer should supply genre/category filtering, not replace MAME’s emulation assessment.

Evidence: [feature-count rule](/Users/tigre/repos/roms-curator/src/core/roms_service.rs:371), [feature parsing](/Users/tigre/repos/roms-curator/src/core/roms_service.rs:410), [hardcoded overrides](/Users/tigre/repos/roms-curator/src/models/roms.rs:103).

### 7. Medium — disk status leaks between sibling disk entries

`extract_chd` stores the previous disk’s status outside the loop. If a later disk omits status, it inherits the previous value rather than taking a fresh default. Explicit good and unrecognized values both become `NoStatus`, which hides distinctions.

The full bundled fixture contains **10 disk records** affected by bad/nodump status inheritance. All ten belong to machines already categorized NotWorking in the independent model; this observation does not establish ten incorrectly excluded playable games.

**Fix:** parse each disk independently with explicit Good/BadDump/NoDump values, required/default attribute handling, optionality, hashes, and merge relationships. Keep dump quality distinct from emulation status: a bad dump label alone does not prove that a game cannot run.

Evidence: [disk parsing](/Users/tigre/repos/roms-curator/src/core/roms_service.rs:436).

### 8. Medium — external input and filesystem errors can panic

Ordinary machines without status are unwrapped during categorization. Unknown status/feature enum values panic. Non-UTF-8 filenames are unwrapped. Directory creation and some report writes panic instead of returning the error promised by their caller’s `Result`.

No missing-status panic was found in the full bundled 0.244 metadata. These are robustness failures for malformed, incomplete, or changed input, rather than demonstrated crashes on that corpus.

**Fix:** validate the root/schema and required machine fields; return errors naming the machine, attribute, and path. Use `PathBuf`/`OsString` for filesystem paths. Use ordinary fallible loops for reporting and propagate I/O errors.

Evidence: [status unwrap](/Users/tigre/repos/roms-curator/src/core/roms_service.rs:91), [enum parsing](/Users/tigre/repos/roms-curator/src/core/roms_service.rs:399), [filename conversion](/Users/tigre/repos/roms-curator/src/core/roms_service.rs:191), [report writes](/Users/tigre/repos/roms-curator/src/models/report.rs:36).

### 9. Medium — simulation writes folders and overstates what it checked

Argument construction creates the destination, probes the report with create/delete, and folder construction creates all four outputs before the simulation guard is reached. Simulation does not inspect nested copy feasibility or prove sufficient space, yet produces success-shaped entries.

**Fix:** construct a plan without mutating the collection or destination. Write only the requested plan/report. Materialization should be a separate operation, and the report should distinguish planning from verification.

Evidence: [argument side effects](/Users/tigre/repos/roms-curator/src/core/args.rs:168), [folder creation](/Users/tigre/repos/roms-curator/src/models/destination_folders.rs:13), [simulation guard](/Users/tigre/repos/roms-curator/src/core/roms_service.rs:286).

### 10. Medium — metadata mismatch and selection reasons are hidden

The program does not expose the XML build/version, require a minimum category match rate, or explain why a game was excluded. Missing CatVer categories trigger a heuristic that treats a machine with a media device and no disk as a system. Category substring exclusions also encode preferences such as dropping board games. Casino exclusion is not a general explicit filter.

CatVer parsing depends on exact section text, does not trim entries, and accepts assignments before the intended category section. Any filesystem entry whose stem matches a machine is eligible, even `robocop.txt`; output names retain original casing.

**Fix:** parse INI sections explicitly, report metadata versions/coverage and unknown categories, validate supported storage forms, and generate a reason for each decision. Unknown category should be visible rather than guessed as a technical machine type. Use canonical machine names for output where required. The provider describes [CatVer as category/version metadata](https://www.progettosnaps.net/catver/).

Evidence: [category parsing](/Users/tigre/repos/roms-curator/src/lib.rs:82), [system heuristic](/Users/tigre/repos/roms-curator/src/core/roms_service.rs:460).

## A simpler design worth keeping

Keep a small Rust library and CLI with this pipeline:

`metadata → selection → source inventory → dependency resolution → plan → copy → verify`

- **Metadata:** immutable machine facts, source build/version, ROM/disk/device requirements, category, description, and original status.
- **Selection:** pure functions producing selected launch targets plus reasons. Named presets and a small allow/deny override file are enough initially.
- **Inventory and resolution:** identify where required bytes live under a declared set mode. Track selected games separately from supporting files and report ambiguity or absence.
- **Plan:** sorted source/destination operations, reasons for retaining dependencies, disk-space estimate, conflicts, and metadata fingerprints. Reuse it for simulation and execution.
- **Materialization:** safe copies into a conventional ROM directory. Keep the frontend’s visible selection in a separate list/playlist rather than trying to encode it in four physical folders.
- **Verification:** check output media using the corresponding MAME build; show verification coverage and limitations.

For a frontend-only goal, metadata selection plus a list export may be sufficient: keep the complete collection in place. For a portable smaller collection, add dependency-complete materialization. These can share the same selection logic.

Whole merged archives will still contain unselected variants. If that is unacceptable, generate a filtered DAT/selection and let a ROM manager rebuild the archive layout. Avoid implementing a new general-purpose archive rebuilder as the first refactor.

### Rust and maintenance cleanup

Use structs with inherent methods or plain functions instead of traits attached to HashMap aliases and boxed writer builders. Return the constructed `Report` directly; its current builder clones a report at the end of copying and discards the clone. Remove unused report fields or actually populate them. Count unique games separately from physical copies and CHD files: currently a CHD-dependent ZIP is also labeled CHD.

Use clap derive for typed CLI arguments, repeated source paths, boolean flags, and corrected help text. Separate CLI validation from library configuration and filesystem writes. Move `rand` and `zip` to development dependencies because production code does not use them. Simplify logging if external YAML configuration is not needed.

Commit `Cargo.lock` for this distributed executable, document a supported Rust version, and build releases with `--release`. The workflow currently uploads debug binaries. Add macOS to CI if it is a supported target. Dependency updates and vulnerability auditing remain unverified until a Rust toolchain is available; age alone is not evidence of an exploitable dependency.

Keep the present XML parser initially. The bundled full XML is 270,216,217 bytes uncompressed, so holding both the input string and DOM warrants measuring peak memory. Stream into compact records if memory or startup time proves a problem. Correctness matters more than parallelizing copies or changing parsers immediately.

## Validation performed and remaining coverage

Source, docs, CI, tests, and benchmarks were inspected. The checkout started clean. No application source was changed and no user ROM collection was accessed.

Local Rust/Cargo is absent, including the usual cargo/rustup locations. **The Rust test suite, build, Clippy, and dependency audit were not run.** No end-to-end MAME execution was possible without a supplied executable and real collection.

A read-only Python characterization independently modeled the relevant Rust branches over `tests/resources/prod_lists_0244.zip`, using only explicitly provided XML attributes. Results:

- XML build: `0.244 (mame0244)`; 44,622 machines and 44,622 category entries.
- Modeled final categories: 10,188 Working, 23,581 NotWorking, 10,780 System, 73 BIOS.
- Comparison with bundled `working_arcade_0244.ini`: 9 additional working names and 1,621 omitted names, matching the expensive integration test’s expected differences. That agreement supports the characterization but is not native execution.
- 749 machines excluded by the extra sound/graphics rule; 35 selected clones with NotWorking parents.
- 10 disk-status inheritance records, all on modeled NotWorking machines.
- Two machines have bad disks skipped by the relationship branch (`quizard`, `quizard2`); both still end NotWorking, so this corpus does not demonstrate a selected bad-disk false positive from that branch.

The small fixture has 11 zero-byte `.zip` placeholders and two zero-byte `.chd` placeholders. The directory called `merged_roms` contains a standalone `elevatora.zip` while its declared parent `elevator` is absent from both the small XML and the archive folder. These fixtures test name-based routing, not actual archive integrity, merged behavior, or playability.

The existing tests provide useful happy-path regression coverage. Add meaningful cases for overlap/symlink safety, conflicting copies, missing dependency media, merged parent/nonworking versus clone/working, split parents, device ROMs, shared/parent CHDs, multiple disk defaults, strict versus playable policies, write failures and exit codes, and read-only planning. Use synthetic archives with arbitrary test bytes and hashes; copyrighted game data is unnecessary. Update tests that currently assume simulation creates output folders.

## Alternatives worth evaluating before investing heavily

There are now credible alternatives and building blocks. Their documentation was checked on the review date; their implementations were not audited or run here.

- **[clrmame](https://mamedev.emulab.it/clrmamepro/binaries/readme.html):** documents XML/XPath and file-list filtering, automatic dependency inclusion according to merge mode, and rebuilding. This is the strongest documented candidate for delegating archive/dependency mechanics while keeping custom category preferences in roms-curator. Check platform fit; the published tools described are Windows executables, with Wine integration discussed for Linux.
- **[MAME Curator](https://github.com/milnet01/mame-curator):** a close match that advertises category filtering, version selection, BIOS copying, a local browser UI, and RetroArch playlist export. Its documented input is a non-merged collection. Establish CHD/shared-device coverage and test on a small sample before treating it as a drop-in replacement.
- **[Retro-Refiner](https://github.com/atkins/retro-refiner):** advertises MAME/CatVer filtering and regional clone selection. Relevant to curation, but its stated features alone do not establish correct merged exports or complete CHD dependency handling.
- **[RomVault](https://wiki.romvault.com/doku.php?id=mame_listxml):** documents MAME XML import and merge settings. A useful collection auditing/rebuilding option; its guide alone does not establish the exact playable-only policy wanted here.

My recommendation is a targeted repair followed by a smaller selection/plan architecture, unless a sample evaluation shows an existing tool already meets the desired workflow. Preserve the useful curation layer and avoid duplicating a mature ROM manager’s full rebuilding engine.

## Recommended sequence and open decisions

1. **Before copying a real collection:** repair overlap/overwrite protection, error propagation, success semantics, and simulation. Add focused safety regressions.
2. **Before claiming a portable curated set:** separate machines from archives, resolve dependencies under an explicit set mode, replace blanket CHD removal, and verify output with MAME.
3. **Then simplify:** use MAME’s overall status by default, configurable category policy and overrides, typed CLI/configuration, and one explainable report/plan.
4. **After correctness:** measure memory/performance, clean up dependencies, and improve release packaging.

The primary decisions are established: playable arcade games including minor issues, merged 0.289 input plus CHDs, smaller self-contained output and a clean frontend list, MAME 0.289 on both Windows and Android, with frontend clone grouping. No further clarification is required for the core design. Category tastes and optional performance/control profiles can be refined later without changing the underlying dependency-safe export design.
