use std::collections::HashSet;

use super::roms::Roms;

/// Why a merged archive is required by a selected game.
///
/// Resolution assumes a merged collection: clone bytes live in the parent archive,
/// and BIOS and device archives are separate files. Disk sharing and delta parents
/// are not resolved here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DependencyKind {
    /// The selected machine is not a clone. Its bytes are in its own archive.
    Own,
    /// `cloneof` parent archive. The clone does not have its own merged archive.
    Parent,
    /// Separate `romof` archive, usually a BIOS or shared ROM parent.
    RomParent,
    /// Device archive reached from `device_ref`. Empty devices are omitted.
    Device,
}

/// One selected game's need for one merged archive.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DependencyEdge {
    /// Selected machine that needs [`Self::archive`].
    pub game: String,
    pub kind: DependencyKind,
    /// Archive file stem. This is the parent name for a clone, not the clone name.
    pub archive: String,
}

/// A `cloneof`, `romof`, or `device_ref` name that is not in the catalog.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct MissingReference {
    /// Selected machine whose dependency walk found [`Self::name`].
    pub game: String,
    pub kind: DependencyKind,
    pub name: String,
}

/// Merged archives required by [`super::roms::Catalog::selected`].
///
/// `archives`, `edges`, `missing`, and `cycles` are sorted. The same inputs always
/// produce the same lists. A cycle is stored once, starting at its lowest name and
/// closing back on that name. The direction is the lexicographically smaller walk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencySet {
    /// Archive stems required by at least one selected game, sorted by name.
    pub archives: Vec<String>,
    /// One entry per selected game and reason. A shared BIOS appears once per game.
    pub edges: Vec<DependencyEdge>,
    /// References that do not name a machine in the catalog.
    pub missing: Vec<MissingReference>,
    /// Each cycle is the visited names plus a repeat of the first name.
    pub cycles: Vec<Vec<String>>,
}

/// Resolves `selected` to merged archives and support files.
///
/// A clone requires its ultimate `cloneof` parent and not its own name. Each `romof`
/// adds that ROM parent's archive. A `device_ref` adds an archive only when that
/// device, or a `cloneof` / `romof` parent of it, has ROM or disk entries. Missing names and cycles
/// are recorded and the walk stops along that edge.
pub fn resolve(roms: &Roms, selected: &[String]) -> DependencySet {
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
        resolve_game(&mut state);
    }

    edges.sort();
    edges.dedup();
    missing.sort();
    missing.dedup();
    cycles.sort();
    cycles.dedup();

    let mut archives: Vec<String> = edges.iter().map(|edge| edge.archive.clone()).collect();
    archives.sort();
    archives.dedup();

    DependencySet {
        archives,
        edges,
        missing,
        cycles,
    }
}

struct Walk<'a> {
    roms: &'a Roms,
    game: &'a str,
    edges: &'a mut Vec<DependencyEdge>,
    missing: &'a mut Vec<MissingReference>,
    cycles: &'a mut Vec<Vec<String>>,
}

enum Ultimate {
    Found(String),
    Missing(String),
    Cycle,
}

fn resolve_game(state: &mut Walk<'_>) {
    let game = state.game.to_string();
    let is_clone = state
        .roms
        .get(&game)
        .and_then(|rom| rom.data.clone_of.as_ref())
        .is_some();
    if is_clone {
        match ultimate_parent(state.roms, &game, state.cycles) {
            Ultimate::Found(archive) => state.push_edge(DependencyKind::Parent, archive),
            Ultimate::Missing(name) => state.push_missing(DependencyKind::Parent, name),
            Ultimate::Cycle => {}
        }
    } else {
        state.push_edge(DependencyKind::Own, game.clone());
    }

    let mut stack = Vec::new();
    let mut seen = HashSet::new();
    // A missing cloneof on the selected game is a parent archive. Devices and
    // ROM parents keep the kind of the edge that reached them.
    walk(state, &mut stack, &mut seen, &game, DependencyKind::Parent);
}

fn walk(
    state: &mut Walk<'_>,
    stack: &mut Vec<String>,
    seen: &mut HashSet<String>,
    current: &str,
    via: DependencyKind,
) {
    if stack.iter().any(|name| name == current) {
        let mut path = stack.clone();
        path.push(current.to_string());
        state.cycles.push(canonicalize_cycle(&path));
        return;
    }
    if !seen.insert(current.to_string()) {
        return;
    }
    let Some(machine) = state.roms.get(current) else {
        return;
    };
    let clone_of = machine.data.clone_of.clone();
    let rom_of = machine.data.rom_of.clone();
    let devices = machine.data.devices.clone();
    stack.push(current.to_string());

    if let Some(parent) = clone_of.clone() {
        if state.roms.contains_key(&parent) {
            walk(state, stack, seen, &parent, via);
        } else {
            state.push_missing(via, parent);
        }
    }
    if let Some(romof) = rom_of {
        if clone_of.as_deref() != Some(romof.as_str()) {
            note_rom_parent(state, &romof);
        }
        if state.roms.contains_key(&romof) {
            walk(state, stack, seen, &romof, DependencyKind::RomParent);
        }
    }
    for device in devices {
        if !state.roms.contains_key(&device) {
            state.push_missing(DependencyKind::Device, device);
            continue;
        }
        for holder in byte_holders(state.roms, &device) {
            match ultimate_parent(state.roms, &holder, state.cycles) {
                Ultimate::Found(archive) => state.push_edge(DependencyKind::Device, archive),
                Ultimate::Missing(name) => state.push_missing(DependencyKind::Device, name),
                Ultimate::Cycle => {}
            }
        }
        walk(state, stack, seen, &device, DependencyKind::Device);
    }

    stack.pop();
}

fn note_rom_parent(state: &mut Walk<'_>, romof: &str) {
    if !state.roms.contains_key(romof) {
        state.push_missing(DependencyKind::RomParent, romof.to_string());
        return;
    }
    match ultimate_parent(state.roms, romof, state.cycles) {
        Ultimate::Found(archive) => state.push_edge(DependencyKind::RomParent, archive),
        Ultimate::Missing(name) => state.push_missing(DependencyKind::RomParent, name),
        Ultimate::Cycle => {}
    }
}

/// Machines reachable by `cloneof` or `romof`, including `start`, that store ROM or disk entries.
fn byte_holders(roms: &Roms, start: &str) -> Vec<String> {
    let mut holders = Vec::new();
    let mut seen = HashSet::new();
    let mut pending = vec![start.to_string()];
    while let Some(name) = pending.pop() {
        if !seen.insert(name.clone()) {
            continue;
        }
        let Some(machine) = roms.get(&name) else {
            continue;
        };
        if !machine.data.roms.is_empty() || !machine.data.chd.is_empty() {
            holders.push(name.clone());
        }
        if let Some(parent) = machine.data.clone_of.clone() {
            pending.push(parent);
        }
        if let Some(parent) = machine.data.rom_of.clone() {
            pending.push(parent);
        }
    }
    holders
}

fn ultimate_parent(roms: &Roms, start: &str, cycles: &mut Vec<Vec<String>>) -> Ultimate {
    let mut current = start.to_string();
    let mut path = vec![current.clone()];
    loop {
        let Some(machine) = roms.get(&current) else {
            return Ultimate::Missing(current);
        };
        let Some(parent) = machine.data.clone_of.clone() else {
            return Ultimate::Found(current);
        };
        if path.iter().any(|name| name == &parent) {
            path.push(parent);
            cycles.push(canonicalize_cycle(&path));
            return Ultimate::Cycle;
        }
        path.push(parent.clone());
        current = parent;
    }
}

/// Rotates the repeated walk so it starts at the lowest name, then picks the
/// lexicographically smaller direction. The returned path repeats its first name.
fn canonicalize_cycle(path: &[String]) -> Vec<String> {
    let end = path.last().expect("cycle path is not empty");
    let start = path.iter().position(|name| name == end).unwrap_or(0);
    let body = &path[start..path.len() - 1];
    if body.is_empty() {
        return vec![end.clone(), end.clone()];
    }
    let forward = rotate_to_min(body);
    let mut reversed: Vec<String> = body.iter().rev().cloned().collect();
    reversed = rotate_to_min(&reversed);
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
    fn push_edge(&mut self, kind: DependencyKind, archive: String) {
        self.edges.push(DependencyEdge {
            game: self.game.to_string(),
            kind,
            archive,
        });
    }

    fn push_missing(&mut self, kind: DependencyKind, name: String) {
        self.missing.push(MissingReference {
            game: self.game.to_string(),
            kind,
            name,
        });
    }
}
