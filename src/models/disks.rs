use std::collections::{HashSet, VecDeque};

use super::roms::{Chd, ChdStatus, Roms};

/// Why a selected game needs a CHD file.
///
/// Dump quality is stored on the edge and does not decide whether the file is kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DiskKind {
    /// The game's own disk, with content that is not already stored by an ancestor.
    Own,
    /// One ancestor copy. The game names it with `merge`, or its sha1 matches.
    Shared,
    /// Parent CHD a clone disk with different content may be stored as a delta against.
    DeltaParent,
    /// Disk stored by a `device_ref` machine.
    Device,
}

/// One selected game's need for one CHD file.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DiskEdge {
    /// Selected machine that needs [`Self::machine`]'s disk.
    pub game: String,
    pub kind: DiskKind,
    /// Machine directory that stores the CHD. This is the ancestor for a share or delta.
    pub machine: String,
    /// Disk name. The CHD stem inside that machine's directory.
    pub name: String,
    pub sha1: Option<String>,
    /// Dump quality of the stored disk. `nodump` and `baddump` are still required.
    pub status: ChdStatus,
}

/// A shared or delta disk that does not resolve to a machine in the catalog.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct MissingDisk {
    /// Selected machine whose disk walk found this gap.
    pub game: String,
    pub kind: DiskKind,
    /// Machine that was expected to store [`Self::name`], when the walk had one.
    pub machine: String,
    pub name: String,
}

/// CHD files required by [`super::roms::Catalog::selected`].
///
/// `disks`, `edges`, `missing`, and `cycles` are sorted. Optional disks are omitted.
/// A missing merge or delta parent is listed here and does not change a game's category.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskSet {
    /// `machine` + `name` pairs required by at least one selected game.
    pub disks: Vec<StoredDisk>,
    /// One entry per selected game and reason. A shared CHD appears once per game.
    pub edges: Vec<DiskEdge>,
    /// Merge and delta references that do not resolve.
    pub missing: Vec<MissingDisk>,
    /// Each cycle is `machine/name` steps plus a repeat of the first step.
    pub cycles: Vec<Vec<String>>,
}

/// A CHD file, independent of which games need it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct StoredDisk {
    pub machine: String,
    pub name: String,
}

/// Resolves `selected` to the CHD files those games need.
///
/// A `merge` attribute or a matching sha1 keeps a single ancestor copy. A clone
/// disk with different content also keeps the same-named non-optional disk on
/// its ultimate `cloneof` parent, because a stored delta CHD requires that
/// parent file. When that parent has disks and none is the same-named
/// non-optional disk, the delta parent is missing. A parent with no disks is not missing.
/// Optional disks are not required. Dump quality does not drop a disk.
pub fn resolve(roms: &Roms, selected: &[String]) -> DiskSet {
    let mut edges = Vec::new();
    let mut missing = Vec::new();
    let mut cycles = Vec::new();

    for game in selected {
        if !roms.contains_key(game) {
            continue;
        }
        let mut state = Walk {
            roms,
            game,
            edges: &mut edges,
            missing: &mut missing,
            cycles: &mut cycles,
        };
        resolve_game(&mut state, game);
    }

    edges.sort();
    edges.dedup();
    missing.sort();
    missing.dedup();
    cycles.sort();
    cycles.dedup();

    let mut disks: Vec<StoredDisk> = edges
        .iter()
        .map(|edge| StoredDisk {
            machine: edge.machine.clone(),
            name: edge.name.clone(),
        })
        .collect();
    disks.sort();
    disks.dedup();

    DiskSet {
        disks,
        edges,
        missing,
        cycles,
    }
}

struct Walk<'a> {
    roms: &'a Roms,
    game: &'a str,
    edges: &'a mut Vec<DiskEdge>,
    missing: &'a mut Vec<MissingDisk>,
    cycles: &'a mut Vec<Vec<String>>,
}

fn resolve_game(state: &mut Walk<'_>, game: &str) {
    require_machine(state, game, DiskKind::Own);
    let devices = state
        .roms
        .get(game)
        .map(|rom| rom.data.devices.clone())
        .unwrap_or_default();
    for device in devices {
        if state.roms.contains_key(&device) {
            require_machine(state, &device, DiskKind::Device);
        } else {
            state.missing.push(MissingDisk {
                game: state.game.to_string(),
                kind: DiskKind::Device,
                machine: device.clone(),
                name: device,
            });
        }
    }
}

fn require_machine(state: &mut Walk<'_>, machine: &str, own_kind: DiskKind) {
    let Some(rom) = state.roms.get(machine) else {
        return;
    };
    let disks = rom.data.chd.clone();
    for disk in disks {
        if disk.optional {
            continue;
        }
        require_disk(state, machine, &disk, own_kind);
    }
}

fn require_disk(state: &mut Walk<'_>, machine: &str, disk: &Chd, own_kind: DiskKind) {
    if let Some(merge) = disk.merge.clone() {
        match locate_merge(state, machine, &merge, &mut Vec::new()) {
            Locate::Found(found) => state.push_edge(DiskKind::Shared, &found),
            Locate::Missing(name) => state.push_missing(DiskKind::Shared, machine, name),
            Locate::Cycle => {}
        }
        return;
    }

    if let Some(sha1) = disk.sha1.clone() {
        if let Some((holder, held)) = shared_sha1(state, machine, &sha1, &disk.name) {
            match identify(state, &holder, &held, &mut Vec::new()) {
                Locate::Found(found) => state.push_edge(DiskKind::Shared, &found),
                Locate::Missing(name) => state.push_missing(DiskKind::Shared, &holder, name),
                Locate::Cycle => {}
            }
            return;
        }
    }

    state.push_edge(
        own_kind,
        &FoundDisk {
            machine: machine.to_string(),
            name: disk.name.clone(),
            sha1: disk.sha1.clone(),
            status: disk.status,
        },
    );

    if state
        .roms
        .get(machine)
        .and_then(|rom| rom.data.clone_of.as_ref())
        .is_some()
    {
        require_delta(state, machine, &disk.name);
    }
}

fn require_delta(state: &mut Walk<'_>, machine: &str, disk_name: &str) {
    let Some(parent) = ultimate_clone_parent(state.roms, machine, state.cycles) else {
        return;
    };
    if parent == machine {
        return;
    }
    let Some(parent_rom) = state.roms.get(&parent) else {
        state.push_missing(DiskKind::DeltaParent, &parent, disk_name.to_string());
        return;
    };
    // A delta base is the parent's non-optional disk of the same name.
    // No parent disks means the clone disk stands alone. Disks that do not
    // include that name are missing rather than guessed.
    if parent_rom.data.chd.is_empty() {
        return;
    }
    let base = parent_rom
        .data
        .chd
        .iter()
        .find(|disk| disk.name == disk_name && !disk.optional)
        .cloned();
    let Some(base) = base else {
        state.push_missing(DiskKind::DeltaParent, &parent, disk_name.to_string());
        return;
    };

    match identify(state, &parent, &base, &mut Vec::new()) {
        Locate::Found(found) => state.push_edge(DiskKind::DeltaParent, &found),
        Locate::Missing(name) => state.push_missing(DiskKind::DeltaParent, &parent, name),
        Locate::Cycle => {}
    }
}

enum Locate {
    Found(FoundDisk),
    Missing(String),
    Cycle,
}

struct FoundDisk {
    machine: String,
    name: String,
    sha1: Option<String>,
    status: ChdStatus,
}

fn identify(state: &mut Walk<'_>, machine: &str, disk: &Chd, stack: &mut Vec<String>) -> Locate {
    if let Some(merge) = disk.merge.clone() {
        locate_merge(state, machine, &merge, stack)
    } else {
        Locate::Found(FoundDisk {
            machine: machine.to_string(),
            name: disk.name.clone(),
            sha1: disk.sha1.clone(),
            status: disk.status,
        })
    }
}

fn locate_merge(state: &mut Walk<'_>, origin: &str, name: &str, stack: &mut Vec<String>) -> Locate {
    let step = format!("{origin}/{name}");
    if stack.iter().any(|seen| seen == &step) {
        let mut path = stack.clone();
        path.push(step);
        state.cycles.push(canonicalize_cycle(&path));
        return Locate::Cycle;
    }
    stack.push(step);

    let mut queue = VecDeque::from(parent_links(state.roms, origin));
    let mut seen = HashSet::new();
    while let Some(machine) = queue.pop_front() {
        if !seen.insert(machine.clone()) {
            continue;
        }
        if let Some(found) = take_named(state, &machine, name, stack) {
            stack.pop();
            return found;
        }
        queue.extend(parent_links(state.roms, &machine));
    }
    if let Some(found) = take_named(state, origin, name, stack) {
        stack.pop();
        return found;
    }
    stack.pop();
    Locate::Missing(name.to_string())
}

fn take_named(
    state: &mut Walk<'_>,
    machine: &str,
    name: &str,
    stack: &mut Vec<String>,
) -> Option<Locate> {
    let disk = state
        .roms
        .get(machine)?
        .data
        .chd
        .iter()
        .find(|disk| disk.name == name)?
        .clone();
    if disk.merge.as_deref() == Some(name) {
        // A disk merged to its own name cannot be the stored copy.
        return None;
    }
    Some(identify(state, machine, &disk, stack))
}

fn shared_sha1(
    state: &mut Walk<'_>,
    start: &str,
    sha1: &str,
    prefer_name: &str,
) -> Option<(String, Chd)> {
    let ultimate = ultimate_clone_parent(state.roms, start, state.cycles);
    let mut nearest: Option<(String, Chd)> = None;
    let mut on_ultimate: Option<(String, Chd)> = None;
    for machine in ancestors(state.roms, start) {
        let Some(rom) = state.roms.get(&machine) else {
            continue;
        };
        let Some(disk) = rom
            .data
            .chd
            .iter()
            .find(|disk| disk.sha1.as_deref() == Some(sha1) && disk.name == prefer_name)
            .or_else(|| {
                rom.data
                    .chd
                    .iter()
                    .find(|disk| disk.sha1.as_deref() == Some(sha1))
            })
        else {
            continue;
        };
        let found = (machine.clone(), disk.clone());
        if ultimate.as_deref() == Some(machine.as_str()) {
            on_ultimate = Some(found);
        } else if nearest.is_none() {
            nearest = Some(found);
        }
    }
    on_ultimate.or(nearest)
}

fn ancestors(roms: &Roms, start: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut queue = VecDeque::from(parent_links(roms, start));
    while let Some(name) = queue.pop_front() {
        if !seen.insert(name.clone()) {
            continue;
        }
        out.push(name.clone());
        queue.extend(parent_links(roms, &name));
    }
    out
}

fn parent_links(roms: &Roms, name: &str) -> Vec<String> {
    let Some(rom) = roms.get(name) else {
        return Vec::new();
    };
    let mut links = Vec::new();
    if let Some(parent) = rom.data.clone_of.clone() {
        links.push(parent);
    }
    if let Some(parent) = rom.data.rom_of.clone() {
        if rom.data.clone_of.as_deref() != Some(parent.as_str()) {
            links.push(parent);
        }
    }
    links
}

fn ultimate_clone_parent(
    roms: &Roms,
    start: &str,
    cycles: &mut Vec<Vec<String>>,
) -> Option<String> {
    let mut current = start.to_string();
    let mut path = vec![current.clone()];
    loop {
        let rom = roms.get(&current)?;
        let Some(parent) = rom.data.clone_of.clone() else {
            return Some(current);
        };
        if path.iter().any(|name| name == &parent) {
            path.push(parent);
            cycles.push(canonicalize_cycle(&path));
            return None;
        }
        if !roms.contains_key(&parent) {
            return Some(parent);
        }
        path.push(parent.clone());
        current = parent;
    }
}

fn canonicalize_cycle(path: &[String]) -> Vec<String> {
    let end = path.last().expect("cycle path is not empty");
    let start = path.iter().position(|name| name == end).unwrap_or(0);
    let body = &path[start..path.len() - 1];
    if body.is_empty() {
        return vec![end.clone(), end.clone()];
    }
    let forward = rotate_to_min(body);
    let reversed = rotate_to_min(&body.iter().rev().cloned().collect::<Vec<_>>());
    let mut chosen = if reversed < forward {
        reversed
    } else {
        forward
    };
    let first = chosen[0].clone();
    chosen.push(first);
    chosen
}

fn rotate_to_min(body: &[String]) -> Vec<String> {
    let min_at = body
        .iter()
        .enumerate()
        .min_by(|left, right| left.1.cmp(right.1))
        .map(|(index, _)| index)
        .unwrap_or(0);
    body[min_at..]
        .iter()
        .chain(&body[..min_at])
        .cloned()
        .collect()
}

impl Walk<'_> {
    fn push_edge(&mut self, kind: DiskKind, found: &FoundDisk) {
        self.edges.push(DiskEdge {
            game: self.game.to_string(),
            kind,
            machine: found.machine.clone(),
            name: found.name.clone(),
            sha1: found.sha1.clone(),
            status: found.status,
        });
    }

    fn push_missing(&mut self, kind: DiskKind, machine: &str, name: String) {
        self.missing.push(MissingDisk {
            game: self.game.to_string(),
            kind,
            machine: machine.to_string(),
            name,
        });
    }
}
