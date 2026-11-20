//! Which stored pairs apply at this derive (spec §1 and §4 of
//! `docs/superpowers/specs/2026-11-19-content-matching-volume-joins-design.md`):
//! the match table, the fingerprints and the libraries are read on the
//! connection derive is given, so a preview sees what a derive would.
//!
//! Who takes part is read from the groups this derive's files and rules make,
//! never from the works the last derive wrote, so the same files, rules and
//! match rows give the same answer. The works are asked only which hold a
//! file by an admin's hand value, which lives nowhere else, and which of
//! those editions a join brought in, since a host's hand values are not its
//! guest's.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use rusqlite::{Connection, OptionalExtension};
use taiyaku_core::{
    Alignment, PageRange, Quality, Run, RunEntry, Slot, SlotSpan, SolverInfo, Source,
    average_run_length, paired_slots, rounded_avg_run, verdict,
};

use super::{Candidate, DeriveReport, Documents, Group, GroupKey};
use crate::librarydoc::{EditionKind, parse_edition_kind};
use crate::matches::{self, BAR, Pair, State};

/// The page hash the match pass's solver reads, as `align` records its own.
const SOLVER_HASH: &str = "dhash-16";

/// A pair that applies, and where its two files sit in this derive's groups:
/// `(group, edition key)`, the pair's smaller digest first.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Edge {
    pub pair: i64,
    pub v: f64,
    pub at: [(GroupKey, String); 2],
}

/// What [`applicable`] finds.
#[derive(Debug, Default)]
pub(super) struct Applicable {
    /// Pairs between the files of two groups.
    pub edges: Vec<Edge>,
    /// Pairs whose files names already put in one group, which join nothing.
    pub together: Vec<Edge>,
    /// Each group a rule's literal series or an admin's hand value holds
    /// where it is: it never moves to join another, though another may join
    /// it, since hand values on the host never stop a join.
    pub held: BTreeSet<GroupKey>,
    /// Of those, each group only an admin's hand values hold, and the series
    /// the works holding them stand in, where that is one: a hand value keeps
    /// a work where it stands (spec §4), so a group whose work followed there
    /// before the admin's edit follows there again, and no other.
    pub stands: BTreeMap<GroupKey, String>,
    /// Each file of a pair an admin split, with the files it is split from:
    /// neither joins a component holding the other (spec §5).
    pub apart: HashMap<String, BTreeSet<String>>,
}

/// What [`apply`] did with the pairs that apply.
#[derive(Debug, Default)]
pub(super) struct Joins {
    /// Each digest that joined another group's work, and the host group it
    /// joined.
    pub moved: BTreeMap<String, GroupKey>,
    /// Every pair that applies whose two files end in one group, its `at`
    /// naming that group.
    pub aligned: Vec<Edge>,
}

/// An edition as a join sees it: its group, and its key there.
type Side = (GroupKey, String);

/// Each group as it ranks for hosting a component, best first: the best kind
/// of its editions in that component (official, fan, raw, then any other or
/// none), its series' age, its work's age, then its key.
type Rank = (u8, Age, (bool, i64), GroupKey);

/// A series' age: its row's id, or after every row by its title where it has
/// none yet, as this derive's own series rows are minted after [`apply`].
type Age = (bool, i64, String);

/// A group's [`Rank`] after its kind: its series' age, then its work's.
type Standing = (Age, (bool, i64));

/// Editions the applied pairs and names link so far, at most one per key: a
/// file whose key the component holds stays out (spec §4, "Edition labels").
/// A group's editions are in more than one once a join leaves some behind,
/// for a key the other side holds or for a Split.
#[derive(Default, Clone)]
struct Component {
    /// The group ranking best by the editions it holds here.
    host: GroupKey,
    groups: Vec<GroupKey>,
    /// Each edition key the host's work will hold, and the group whose
    /// edition it is.
    keys: BTreeMap<String, GroupKey>,
    /// The edges that linked it, as indices into the sorted edges, in order.
    applied: Vec<usize>,
}

/// What linking reads and never changes.
struct Ground<'g> {
    groups: &'g BTreeMap<GroupKey, Group>,
    edges: &'g [Edge],
    held: &'g BTreeSet<GroupKey>,
    apart: &'g HashMap<String, BTreeSet<String>>,
    /// Each edition's kind, as [`kind_rank`] ranks it.
    kinds: BTreeMap<Side, u8>,
    /// Each group's [`Rank`] after its kind.
    standing: BTreeMap<GroupKey, Standing>,
}

impl<'g> Ground<'g> {
    /// The digests of an edition's files.
    fn files(&self, (group, key): &Side) -> Vec<&'g str> {
        self.groups
            .get(group)
            .and_then(|group| group.editions.get(key))
            .into_iter()
            .flatten()
            .map(|candidate| candidate.sha256.as_str())
            .collect()
    }

    /// Whether a file of `one` is split from a file of any of `others`.
    fn parted<'s>(&self, one: &Side, others: impl IntoIterator<Item = &'s Side>) -> bool {
        let from: Vec<&BTreeSet<String>> = self
            .files(one)
            .into_iter()
            .filter_map(|file| self.apart.get(file))
            .collect();
        !from.is_empty()
            && others.into_iter().any(|other| {
                self.files(other)
                    .into_iter()
                    .any(|file| from.iter().any(|apart| apart.contains(file)))
            })
    }

    /// How `group` ranks by the editions `keys` holds of it.
    fn rank(&self, keys: &BTreeMap<String, GroupKey>, group: &GroupKey) -> Rank {
        let kind = keys
            .iter()
            .filter(|(_, of)| *of == group)
            .map(|(key, _)| {
                self.kinds
                    .get(&(group.clone(), key.clone()))
                    .copied()
                    .unwrap_or(3)
            })
            .min()
            .unwrap_or(3);
        let (age, work) = self.standing[group].clone();
        (kind, age, work, group.clone())
    }

    /// The component holding `keys`, linked by `applied`.
    fn component(&self, keys: BTreeMap<String, GroupKey>, mut applied: Vec<usize>) -> Component {
        let mut groups: Vec<GroupKey> = Vec::new();
        for group in keys.values() {
            if !groups.contains(group) {
                groups.push(group.clone());
            }
        }
        let host = groups
            .iter()
            .min_by_key(|group| self.rank(&keys, group))
            .cloned()
            .unwrap_or_default();
        applied.sort_unstable();
        applied.dedup();
        Component {
            host,
            groups,
            keys,
            applied,
        }
    }

    /// The edges of `applied` whose two editions are both in `within`.
    fn within<'a>(
        &'a self,
        applied: &'a [usize],
        within: &'a BTreeSet<Side>,
    ) -> impl Iterator<Item = usize> + 'a {
        applied
            .iter()
            .copied()
            .filter(|one| self.edges[*one].at.iter().all(|side| within.contains(side)))
    }
}

/// Each edition `keys` holds.
fn sides(keys: &BTreeMap<String, GroupKey>) -> impl Iterator<Item = Side> + '_ {
    keys.iter().map(|(key, group)| (group.clone(), key.clone()))
}

/// The components the pairs link so far.
#[derive(Default)]
struct Linking {
    components: Vec<Component>,
    /// Each edition by the component it is in.
    of: BTreeMap<Side, usize>,
    /// Each edition a join left behind that no pair has taken in since.
    left: BTreeSet<Side>,
    /// Each group a join took part in, even one a join then left behind: it
    /// stays where it is rather than follow its directory.
    touched: BTreeSet<GroupKey>,
}

impl Linking {
    /// Link the two components edge `at` names, the losing side's editions
    /// joining the winner's host, unless that would move a held group, put
    /// two files under one key or a split pair's two files in one component,
    /// or part a group's editions left behind.
    fn link(&mut self, ground: &Ground<'_>, at: usize) {
        let edge = &ground.edges[at];
        let (ca, cb) = (self.of[&edge.at[0]], self.of[&edge.at[1]]);
        if ca == cb {
            return;
        }
        let rank = |c: &Component| ground.rank(&c.keys, &c.host);
        let (win, lose) = if rank(&self.components[ca]) <= rank(&self.components[cb]) {
            (ca, cb)
        } else {
            (cb, ca)
        };
        let moving = if lose == ca { &edge.at[0] } else { &edge.at[1] };
        let (winning, losing) = (&self.components[win], &self.components[lose]);
        let won: Vec<Side> = sides(&winning.keys).collect();
        let lost: BTreeSet<Side> = sides(&losing.keys).collect();
        let taken = |side: &Side| winning.keys.contains_key(&side.1);
        let named = |side: &Side| {
            losing
                .applied
                .iter()
                .any(|one| ground.edges[*one].at.contains(side))
        };
        // A held group never moves, though it may host. Two files under one
        // key are copies under one label, which content matching does not
        // settle: the moving file needs its key free, and another edition of
        // its side whose key is taken stays behind, unless a join already made
        // names its file, which leaving it there would undo.
        if losing
            .groups
            .iter()
            .any(|group| ground.held.contains(group))
            || taken(moving)
            || ground.parted(moving, &won)
            || lost.iter().any(|side| taken(side) && named(side))
        {
            return;
        }
        // Of the losing side, only what the moving file reaches joins, never
        // through a file split from one on the winning side, or whose key that
        // side holds. The rest is left behind, in parts its edges and names
        // still tie together.
        let free: BTreeSet<Side> = lost
            .iter()
            .filter(|side| !taken(side) && !ground.parted(side, &won))
            .cloned()
            .collect();
        let joining = reach(
            ground.edges,
            &losing.applied,
            &free,
            BTreeSet::from([moving.clone()]),
        );
        let mut rest: BTreeSet<Side> = lost.difference(&joining).cloned().collect();
        let mut parts = Vec::new();
        while let Some(first) = rest.first().cloned() {
            let part = reach(
                ground.edges,
                &losing.applied,
                &rest,
                BTreeSet::from([first]),
            );
            rest.retain(|side| !part.contains(side));
            parts.push(part);
        }
        let mut keys = winning.keys.clone();
        keys.extend(
            joining
                .iter()
                .map(|(group, key)| (key.clone(), group.clone())),
        );
        let mut applied = winning.applied.clone();
        applied.extend(ground.within(&losing.applied, &joining));
        applied.push(at);
        let grown = ground.component(keys, applied);
        // Names tie a group's editions left behind: a part joins each
        // component holding editions left behind of one of its groups, the
        // winner's too, and so whatever that one holds. Where that would put
        // two files under one key, a split pair's files together or a held
        // group under another host, the edge applies nothing, so a group's
        // editions left behind are never in two components.
        let mut unions: Vec<(BTreeSet<usize>, Vec<usize>)> = Vec::new();
        for (index, part) in parts.iter().enumerate() {
            let mut union: (BTreeSet<usize>, Vec<usize>) = (
                self.left
                    .iter()
                    .filter(|side| part.iter().any(|one| one.0 == side.0))
                    .map(|side| self.of[side])
                    .filter(|component| *component != lose)
                    .collect(),
                vec![index],
            );
            unions.retain(|(components, members)| {
                if components.is_disjoint(&union.0) {
                    return true;
                }
                union.0.extend(components);
                union.1.extend(members);
                false
            });
            unions.push(union);
        }
        let mut planned = Vec::new();
        for (components, members) in &unions {
            let mut all: Vec<Side> = Vec::new();
            let mut applied = Vec::new();
            for &one in components {
                let component = if one == win {
                    &grown
                } else {
                    &self.components[one]
                };
                all.extend(sides(&component.keys));
                applied.extend(&component.applied);
            }
            for &member in members {
                all.extend(parts[member].iter().cloned());
                applied.extend(ground.within(&losing.applied, &parts[member]));
            }
            let mut keys = BTreeMap::new();
            for (n, side) in all.iter().enumerate() {
                if keys.insert(side.1.clone(), side.0.clone()).is_some()
                    || ground.parted(side, &all[n + 1..])
                {
                    return;
                }
            }
            let component = ground.component(keys, applied);
            if component
                .groups
                .iter()
                .any(|group| ground.held.contains(group) && *group != component.host)
            {
                return;
            }
            planned.push((components.clone(), component));
        }
        let took_part: Vec<GroupKey> = winning
            .groups
            .iter()
            .chain(&losing.groups)
            .cloned()
            .collect();
        self.touched.extend(took_part);
        for side in &joining {
            self.left.remove(side);
            self.of.insert(side.clone(), win);
        }
        for part in &parts {
            self.left.extend(part.iter().cloned());
        }
        self.components[win] = grown;
        self.components[lose] = Component::default();
        for (components, component) in planned {
            let into = if components.contains(&win) {
                win
            } else if let Some(&first) = components.first() {
                first
            } else {
                self.components.push(Component::default());
                self.components.len() - 1
            };
            for &one in &components {
                self.components[one] = Component::default();
            }
            for side in sides(&component.keys) {
                self.of.insert(side, into);
            }
            self.components[into] = component;
        }
    }
}

/// Join the groups the pairs that apply link (spec §4): each component
/// becomes its host's group, the host keeping its series, volume and hub.
/// Edges are taken best first (V, then a pair within one series, then the
/// older series of the two, then the newer, then the digests), so the outcome
/// depends on neither scan order nor the order the pairs were stored in.
/// The groups no join touched then [`follow`] their directories.
///
/// # Errors
///
/// If the store refuses a read.
pub(super) fn apply(
    conn: &Connection,
    mut groups: BTreeMap<GroupKey, Group>,
    report: &mut DeriveReport,
) -> Result<(BTreeMap<GroupKey, Group>, Joins), String> {
    let _snapshot =
        crate::one_snapshot(conn).map_err(|e| format!("reading what joins apply: {e}"))?;
    let Applicable {
        mut edges,
        mut together,
        held,
        stands,
        apart,
    } = applicable(conn, &groups)?;
    if edges.is_empty() && together.is_empty() {
        return Ok((groups, Joins::default()));
    }
    let ages = ages(conn, &groups, edges.iter().chain(&together))?;
    let standing = standings(conn, &groups, &edges, &ages)?;
    let order = |edge: &Edge| {
        let [one, two] = edge.at.each_ref().map(|(group, _)| &ages[&group.0]);
        (
            std::cmp::Reverse(OrdF64(edge.v)),
            edge.at[0].0.0 != edge.at[1].0.0,
            one.min(two).clone(),
            one.max(two).clone(),
            edge.at
                .each_ref()
                .map(|side| file(&groups, side).to_owned()),
        )
    };
    edges.sort_by_cached_key(order);
    together.sort_by_cached_key(order);

    // Each group an edge links, a component of its own to begin with.
    let mut linking = Linking::default();
    let mut kinds = BTreeMap::new();
    let mut seen = BTreeSet::new();
    for (group, _) in edges.iter().flat_map(|edge| &edge.at) {
        if !seen.insert(group) {
            continue;
        }
        let mut keys = BTreeMap::new();
        for (key, candidates) in &groups[group].editions {
            let side = (group.clone(), key.clone());
            linking.of.insert(side.clone(), linking.components.len());
            kinds.insert(
                side,
                kind_rank(super::resolved_metadata(candidates).kind.as_deref()),
            );
            keys.insert(key.clone(), group.clone());
        }
        linking.components.push(Component {
            host: group.clone(),
            groups: vec![group.clone()],
            keys,
            applied: Vec::new(),
        });
    }
    let ground = Ground {
        groups: &groups,
        edges: &edges,
        held: &held,
        apart: &apart,
        kinds,
        standing,
    };
    for at in 0..edges.len() {
        linking.link(&ground, at);
    }

    // What each component but a whole group alone writes: its host, the hub
    // it takes where its work is new or its own hub left (the host's best
    // edition here, in a part left behind hosting nothing too), and each guest
    // edition with the pair that marks it, the first linking edge naming its
    // file, else the first naming another file of its group, which marks
    // nothing.
    let Linking {
        components,
        mut touched,
        ..
    } = linking;
    let mut joined = Vec::new();
    for component in components {
        let Component {
            host,
            groups: linked,
            keys,
            applied,
        } = component;
        let whole = groups
            .get(&host)
            .is_some_and(|group| group.editions.len() == keys.len());
        if linked.is_empty() || (linked.len() == 1 && whole) {
            continue;
        }
        touched.extend(linked);
        let hub_hint = keys
            .iter()
            .filter(|(_, group)| **group == host)
            .min_by_key(|(key, group)| {
                let kind = ground.kinds.get(&((*group).clone(), (*key).clone()));
                (kind.copied().unwrap_or(3), *key)
            })
            .map(|(key, _)| key.clone());
        let mut guests = Vec::new();
        for (key, from) in keys {
            if from == host {
                continue;
            }
            let side = (from.clone(), key.clone());
            let first = applied
                .iter()
                .find(|one| edges[**one].at.contains(&side))
                .or_else(|| {
                    applied
                        .iter()
                        .find(|one| edges[**one].at.iter().any(|(group, _)| *group == from))
                })
                .or(applied.first());
            if let Some(first) = first {
                guests.push((from, key, edges[*first].pair));
            }
        }
        joined.push((host, hub_hint, guests));
    }

    // Every guest leaves its group before any arrives, so a host can take in
    // a key one of its own editions leaves for another component.
    let mut moved = BTreeMap::new();
    let mut ends: BTreeMap<Side, GroupKey> = BTreeMap::new();
    let mut tallies: Tallies = BTreeMap::new();
    let mut arriving = Vec::new();
    for (host, _, guests) in &joined {
        let series = groups[host].series.clone();
        for (from, key, pair) in guests {
            let mut moving = groups
                .get_mut(from)
                .and_then(|group| group.editions.remove(key))
                .unwrap_or_default();
            for candidate in &mut moving {
                candidate.joined_by = Some(*pair);
                moved.insert(candidate.sha256.clone(), host.clone());
                report.joined += 1;
                let (root, at, own) = origin(candidate, &from.0);
                *tallies
                    .entry((root, at.to_owned(), own.to_owned()))
                    .or_default()
                    .entry(series.clone())
                    .or_default() += 1;
            }
            ends.insert((from.clone(), key.clone()), host.clone());
            arriving.push((host.clone(), key.clone(), moving));
        }
    }
    for (host, key, moving) in arriving {
        if let Some(into) = groups.get_mut(&host) {
            into.editions.insert(key, moving);
        }
    }
    for (host, hub_hint, _) in joined {
        if let Some(into) = groups.get_mut(&host) {
            into.hub_hint = hub_hint;
        }
    }
    groups.retain(|_, group| !group.editions.is_empty());
    let followed = follow(&mut groups, &tallies, &touched, (&held, &stands), report);

    let end = |(group, key): &(GroupKey, String)| {
        ends.get(&(group.clone(), key.clone()))
            .or_else(|| followed.get(group))
            .cloned()
            .unwrap_or_else(|| group.clone())
    };
    let mut aligned = Vec::new();
    for edge in edges.into_iter().chain(together) {
        let [one, two] = edge.at.each_ref().map(end);
        if one == two {
            let [(_, one_key), (_, two_key)] = edge.at;
            aligned.push(Edge {
                pair: edge.pair,
                v: edge.v,
                at: [(one, one_key), (two, two_key)],
            });
        }
    }
    Ok((groups, Joins { moved, aligned }))
}

/// What `reached` reaches within `within`, through `linked` (indices into
/// `edges`) and through names, which tie the editions of one group, taken
/// until nothing more is reached, so the order of the edges does not matter.
fn reach(
    edges: &[Edge],
    linked: &[usize],
    within: &BTreeSet<(GroupKey, String)>,
    mut reached: BTreeSet<(GroupKey, String)>,
) -> BTreeSet<(GroupKey, String)> {
    loop {
        let before = reached.len();
        for &one in linked {
            let [a, b] = &edges[one].at;
            for (from, to) in [(a, b), (b, a)] {
                if reached.contains(from) && within.contains(to) {
                    reached.insert(to.clone());
                }
            }
        }
        for side in within {
            if reached.iter().any(|(group, _)| *group == side.0) {
                reached.insert(side.clone());
            }
        }
        if reached.len() == before {
            return reached;
        }
    }
}

/// Move each edition a join brought in, or holding a file this derive joined,
/// off a work this derive did not give its key, onto the work it placed one of
/// its files on under that key, with its readers' state (spec §4,
/// [`crate::fold::move_edition`]). By path, as new bytes leave the old digest
/// on the stale member; by key, as the move does not ask whether the target
/// holds one. A refusal is counted. A work whose hub moved, or is a guest's
/// now, takes the hub its `content_key` was hashed over (`hubs`), and its hand
/// document follows.
///
/// # Errors
///
/// If the store refuses a read or a write.
pub(super) fn move_left_behind(
    conn: &Connection,
    written: &BTreeSet<i64>,
    written_editions: &BTreeSet<(i64, String)>,
    hubs: &BTreeMap<i64, String>,
    joined: &BTreeMap<String, GroupKey>,
    documents: Documents,
    report: &mut DeriveReport,
) -> Result<(), String> {
    let failed = |e: rusqlite::Error| format!("reading which editions a join left behind: {e}");
    let rows: Vec<(i64, String, bool, Option<i64>, String, String)> = conn
        .prepare(
            "SELECT e.work_id, e.key, e.joined_by IS NOT NULL, m.root_id, m.path, m.sha256
               FROM edition e JOIN member m ON m.edition_id = e.id
              ORDER BY e.work_id, e.key, m.ordinal",
        )
        .and_then(|mut stmt| {
            stmt.query_map([], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            })?
            .collect()
        })
        .map_err(failed)?;
    let given = |work: i64, key: &str| written_editions.contains(&(work, key.to_owned()));
    let placed: HashMap<(i64, &str, &str), i64> = rows
        .iter()
        .filter(|(work, key, ..)| given(*work, key))
        .filter_map(|(work, key, _, root, path, _)| {
            Some((((*root)?, path.as_str(), key.as_str()), *work))
        })
        .collect();
    // Each edition left behind: whether a join brought it or its file in, and
    // the work its first member placed under its key is on.
    // The same of each work no group was written onto, by work.
    let mut left: BTreeMap<(i64, &str), (bool, Option<i64>)> = BTreeMap::new();
    let mut strays: BTreeMap<i64, BTreeMap<&str, (bool, Option<i64>)>> = BTreeMap::new();
    for (work, key, marked, root, path, sha256) in &rows {
        if given(*work, key) {
            continue;
        }
        let (joins, into) = if written.contains(work) {
            left.entry((*work, key.as_str())).or_default()
        } else {
            strays
                .entry(*work)
                .or_default()
                .entry(key.as_str())
                .or_default()
        };
        *joins |= *marked || joined.contains_key(sha256);
        if into.is_none() {
            *into = root.and_then(|root| placed.get(&(root, path.as_str(), key.as_str())).copied());
        }
    }
    for ((from, key), (joins, into)) in left {
        let (true, Some(into)) = (joins, into) else {
            continue;
        };
        let hub = hubs.get(&from);
        let tx = crate::Nested::open(conn, "derive_move_left_behind")
            .map_err(|e| format!("starting to move edition `{key}` off work {from}: {e}"))?;
        // Before the move, while the edition the hub names, which addresses
        // the hand document, still stands.
        let held_at = match documents {
            Documents::Reconcile => crate::hand::path_for(&tx, from)?,
            Documents::Skip => None,
        };
        match crate::fold::move_edition(&tx, from, into, key)? {
            crate::fold::Outcome::Folded(moved) => {
                if let Some(hub) = hub {
                    tx.execute(
                        "UPDATE work SET hub = ?2 WHERE id = ?1 AND hub = ?3",
                        rusqlite::params![from, hub, key],
                    )
                    .map_err(|e| format!("giving work {from} a hub it holds: {e}"))?;
                }
                tx.commit().map_err(|e| {
                    format!("committing the move of edition `{key}` off work {from}: {e}")
                })?;
                // A new hub can sit under another root, which moves the
                // document's address; after the commit, as `write_work`
                // renames after a rekey, so a rollback never strands it.
                if let Some(old) = held_at {
                    crate::hand::rename_from(conn, from, &old)?;
                }
                report.moved_editions += 1;
                report.kept_editions = report.kept_editions.saturating_sub(1);
                report.count_moved(&moved);
            }
            crate::fold::Outcome::Refused(why) => {
                report.unmoved_editions += 1;
                report.unfoldable.push(why);
            }
        }
    }
    let held: Vec<(i64, String, bool)> = conn
        .prepare(
            "SELECT w.id, w.hub, EXISTS (SELECT 1 FROM edition e WHERE e.work_id = w.id
                                           AND e.key = w.hub AND e.joined_by IS NULL)
               FROM work w",
        )
        .and_then(|mut stmt| {
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .collect()
        })
        .map_err(failed)?;
    for (from, hub, own) in &held {
        let Some(given) = hubs.get(from).filter(|given| !own && *given != hub) else {
            continue;
        };
        let held_at = match documents {
            Documents::Reconcile => crate::hand::path_for(conn, *from)?,
            Documents::Skip => None,
        };
        conn.execute(
            "UPDATE work SET hub = ?2 WHERE id = ?1",
            rusqlite::params![from, given],
        )
        .map_err(|e| format!("giving work {from} a hub of its own: {e}"))?;
        if let Some(old) = held_at {
            crate::hand::rename_from(conn, *from, &old)?;
        }
    }
    // [`super::superseded`] folds a work no group was written onto into the
    // one work holding all its files, so of one whose files went to several,
    // each edition bound elsewhere than its hub's moves first.
    let hubs: HashMap<i64, String> = held.into_iter().map(|(id, hub, _)| (id, hub)).collect();
    for (from, editions) in strays {
        let Some(hub) = hubs.get(&from) else {
            continue;
        };
        let bound = editions.get(hub.as_str()).and_then(|(_, into)| *into);
        for (key, (joins, into)) in editions {
            let (true, Some(into)) = (joins, into) else {
                continue;
            };
            if key == hub || bound == Some(into) {
                continue;
            }
            match crate::fold::move_edition(conn, from, into, key)? {
                crate::fold::Outcome::Folded(moved) => {
                    report.moved_editions += 1;
                    report.count_moved(&moved);
                }
                crate::fold::Outcome::Refused(why) => {
                    report.unmoved_editions += 1;
                    report.unfoldable.push(why);
                }
            }
        }
    }
    Ok(())
}

/// How many of the files that joined from each [`origin`] went to each
/// series. The origin names the series a file's name gave it, so a directory
/// holding two series leads only the one whose files joined.
type Tallies = BTreeMap<(i64, String, String), BTreeMap<String, usize>>;

/// Where a file comes from, as [`Tallies`] keys it: its root, its parent
/// directory under that root, and `series`, the one its name gave it.
fn origin<'a>(candidate: &'a Candidate, series: &'a str) -> (i64, &'a str, &'a str) {
    (
        candidate.root_id,
        super::parent_of(&candidate.rel_path),
        series,
    )
}

/// Move each group no join touched into the series its files' origins lead
/// to (spec §4, "Directories follow"), keeping its volume. An origin leads
/// to the series more of its joined files went to than any other, so a tie
/// moves nothing. A group moves when every file of it, read under the
/// group's own series, leads to one series that lacks its volume (one it
/// holds means a match was expected and not found), and nothing holds it but
/// hand values on a work standing there (`stands`). Groups go in key order.
/// Answers each group that followed, by its key before and after.
fn follow(
    groups: &mut BTreeMap<GroupKey, Group>,
    tallies: &Tallies,
    touched: &BTreeSet<GroupKey>,
    (held, stands): (&BTreeSet<GroupKey>, &BTreeMap<GroupKey, String>),
    report: &mut DeriveReport,
) -> BTreeMap<GroupKey, GroupKey> {
    let leads: BTreeMap<(i64, &str, &str), &String> = tallies
        .iter()
        .filter_map(|((root, at, own), counts)| {
            let (series, most) = counts.iter().max_by_key(|(_, n)| **n)?;
            counts
                .iter()
                .all(|(other, n)| other == series || n < most)
                .then_some(((*root, at.as_str(), own.as_str()), series))
        })
        .collect();
    let mut followed = BTreeMap::new();
    let keys: Vec<GroupKey> = groups.keys().cloned().collect();
    for key in keys {
        if touched.contains(&key) {
            continue;
        }
        let group = &groups[&key];
        let Some(volume) = group.volume else {
            continue;
        };
        let mut leading = group
            .editions
            .values()
            .flatten()
            .map(|candidate| leads.get(&origin(candidate, &group.series)).copied());
        let Some(Some(series)) = leading.next() else {
            continue;
        };
        if !leading.all(|lead| lead == Some(series))
            || (held.contains(&key) && stands.get(&key) != Some(series))
        {
            continue;
        }
        let to: GroupKey = (series.clone(), Some(volume.to_bits()), None);
        if groups.contains_key(&to) {
            continue;
        }
        if let Some(mut group) = groups.remove(&key) {
            group.series.clone_from(series);
            groups.insert(to.clone(), group);
            followed.insert(key, to);
            report.followed += 1;
        }
    }
    followed
}

/// Store each pair in `aligned` as the solver's alignment on the work its two
/// files now share (spec §4, "Alignments"), in whole pages, from the work's
/// hub, as a document's spokes start (`Anchor::Hub`), else from the
/// higher-ranked kind, else from the smaller key. A person's alignment for
/// the pair keeps the solver's out ([`crate::import::put_solver_alignment`]),
/// and a pair whose page count on either side is not its edition's is
/// counted instead ([`DeriveReport::unwritten_alignments`]).
///
/// # Errors
///
/// If the store refuses a read or a write, or `aligned` names a group by a
/// key `work_ids` lacks: both key a group as the map [`apply`] returned does,
/// never as it was before, so a miss is a defect and not a group to skip.
pub(super) fn write_alignments(
    conn: &Connection,
    aligned: &[Edge],
    work_ids: &BTreeMap<GroupKey, i64>,
    report: &mut DeriveReport,
) -> Result<(), String> {
    if aligned.is_empty() {
        return Ok(());
    }
    let pairs: HashMap<i64, Pair> = matches::pairs(conn)?
        .into_iter()
        .map(|pair| (pair.id, pair))
        .collect();
    for edge in aligned {
        let [(group, a_key), (_, b_key)] = &edge.at;
        let work_id = *work_ids
            .get(group)
            .ok_or_else(|| format!("no work was written for pair {}'s group", edge.pair))?;
        // A pair deleted since [`apply`] read it has no runs left to store,
        // and the next derive no longer applies it.
        let Some(pair) = pairs.get(&edge.pair) else {
            continue;
        };
        if (
            edition_pages(conn, work_id, a_key)?,
            edition_pages(conn, work_id, b_key)?,
        ) != (i64::from(pair.new.a_pages), i64::from(pair.new.b_pages))
        {
            report.unwritten_alignments += 1;
            continue;
        }
        let from_is_a = starts_at_a(conn, work_id, a_key, b_key)?;
        let (from, to) = if from_is_a {
            (a_key, b_key)
        } else {
            (b_key, a_key)
        };
        crate::import::put_solver_alignment(
            conn,
            work_id,
            &alignment_of(pair, from_is_a, from, to),
        )?;
    }
    Ok(())
}

/// The pages edition `key` of `work_id` holds, over all its members.
fn edition_pages(conn: &Connection, work_id: i64, key: &str) -> Result<i64, String> {
    conn.prepare_cached(
        "SELECT coalesce(sum(m.pages), 0) FROM member m JOIN edition e ON e.id = m.edition_id
          WHERE e.work_id = ?1 AND e.key = ?2",
    )
    .and_then(|mut stmt| stmt.query_row(rusqlite::params![work_id, key], |r| r.get(0)))
    .map_err(|e| format!("counting the pages of edition `{key}` of work {work_id}: {e}"))
}

/// Whether the alignment between editions `a` and `b` of `work_id` starts at
/// `a`: see [`write_alignments`].
fn starts_at_a(conn: &Connection, work_id: i64, a: &str, b: &str) -> Result<bool, String> {
    let failed =
        |e: rusqlite::Error| format!("reading where an alignment of work {work_id} starts: {e}");
    let hub: String = conn
        .query_row("SELECT hub FROM work WHERE id = ?1", [work_id], |r| {
            r.get(0)
        })
        .map_err(failed)?;
    if hub == a || hub == b {
        return Ok(hub == a);
    }
    let mut kind = conn
        .prepare_cached("SELECT kind FROM edition WHERE work_id = ?1 AND key = ?2")
        .map_err(failed)?;
    let mut rank = |key: &str| -> Result<u8, String> {
        let stored: Option<Option<String>> = kind
            .query_row(rusqlite::params![work_id, key], |r| r.get(0))
            .optional()
            .map_err(failed)?;
        Ok(kind_rank(stored.flatten().as_deref()))
    };
    Ok((rank(a)?, a) <= (rank(b)?, b))
}

/// The solver's alignment of `pair` from edition `from_key` to `to_key`: the
/// pass's whole-page runs, turned round when the from side is the pair's b,
/// and an unpaired run for each stretch of the from side no run covers, so
/// every page reads on one run and the figures are those of the runs stored.
fn alignment_of(pair: &Pair, from_is_a: bool, from_key: &str, to_key: &str) -> Alignment {
    let new = &pair.new;
    let (pages, mut runs): (u32, Vec<(PageRange, PageRange)>) = if from_is_a {
        (new.a_pages, new.runs.clone())
    } else {
        (new.b_pages, new.runs.iter().map(|&(a, b)| (b, a)).collect())
    };
    runs.sort_by_key(|(from, _)| from.start);
    let mut filled = Vec::with_capacity(2 * runs.len() + 1);
    let mut next = 0;
    for (from, to) in runs {
        if from.start > next {
            filled.push(Run::gap(PageRange::new(next, from.start - 1)));
        }
        next = from.end + 1;
        filled.push(Run::paired(from, to));
    }
    if next < pages {
        filled.push(Run::gap(PageRange::new(next, pages - 1)));
    }
    let whole = |range: PageRange| SlotSpan {
        start: Slot {
            page: range.start,
            half: None,
        },
        end: Slot {
            page: range.end,
            half: None,
        },
    };
    Alignment {
        from: from_key.to_owned(),
        to: to_key.to_owned(),
        by: Source::Solver,
        solver: new.floor.map(|floor| SolverInfo {
            hash: SOLVER_HASH.to_owned(),
            floor,
        }),
        quality: Some(Quality {
            verdict: verdict(&filled, usize::try_from(pages).unwrap_or(usize::MAX)),
            slots: pages,
            paired: paired_slots(&filled),
            avg_run: rounded_avg_run(average_run_length(&filled)),
        }),
        runs: filled
            .iter()
            .map(|run| RunEntry {
                a: whole(run.a),
                b: run.b.map(whole),
                margin: None,
                by: None,
                note: None,
            })
            .collect(),
    }
}

/// An edition kind's rank, best first: official, fan, raw, then any other or
/// none.
fn kind_rank(kind: Option<&str>) -> u8 {
    kind.and_then(|kind| parse_edition_kind(kind).ok())
        .map_or(3, |kind| match kind {
            EditionKind::Official => 0,
            EditionKind::Fan => 1,
            EditionKind::Raw => 2,
        })
}

/// A V, ordered by `f64::total_cmp`: a stored V is never NaN.
#[derive(Clone, Copy, PartialEq)]
struct OrdF64(f64);

impl Eq for OrdF64 {}

impl PartialOrd for OrdF64 {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for OrdF64 {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.total_cmp(&other.0)
    }
}

/// The digest of the one file an edge seats at `(group, key)`: [`applicable`]
/// seats only the only file of its edition.
fn file<'g>(groups: &'g BTreeMap<GroupKey, Group>, (group, key): &(GroupKey, String)) -> &'g str {
    groups
        .get(group)
        .and_then(|group| group.editions.get(key))
        .and_then(|candidates| candidates.first())
        .map_or("", |candidate| candidate.sha256.as_str())
}

/// The age of each series the edges' groups name, read and never minted.
fn ages<'e>(
    conn: &Connection,
    groups: &BTreeMap<GroupKey, Group>,
    edges: impl Iterator<Item = &'e Edge>,
) -> Result<BTreeMap<String, Age>, String> {
    let mut stmt = conn
        .prepare_cached("SELECT id FROM series WHERE sort_title = ?1 ORDER BY id LIMIT 1")
        .map_err(|e| format!("reading the series' ages: {e}"))?;
    let mut ages = BTreeMap::new();
    for (group, _) in edges.flat_map(|edge| &edge.at) {
        let Some(group) = groups.get(group) else {
            continue;
        };
        if ages.contains_key(&group.series) {
            continue;
        }
        let id: Option<i64> = stmt
            .query_row([&group.series], |r| r.get(0))
            .optional()
            .map_err(|e| format!("reading the age of series `{}`: {e}", group.series))?;
        ages.insert(
            group.series.clone(),
            (id.is_none(), id.unwrap_or_default(), group.series.clone()),
        );
    }
    Ok(ages)
}

/// Each group's [`Rank`] after its kind, for each group an edge links. A
/// work's age is the row [`super::held_work`] would write the group onto,
/// which only reads.
fn standings(
    conn: &Connection,
    groups: &BTreeMap<GroupKey, Group>,
    edges: &[Edge],
    ages: &BTreeMap<String, Age>,
) -> Result<BTreeMap<GroupKey, Standing>, String> {
    let placed = super::Placed::of(groups)?;
    let mut standing = BTreeMap::new();
    for (key, _) in edges.iter().flat_map(|edge| &edge.at) {
        if standing.contains_key(key) {
            continue;
        }
        let group = &groups[key];
        let age = ages[&group.series].clone();
        let work = match age {
            (false, series_id, _) => {
                super::held_work(conn, series_id, group, &placed)?.map(|(id, _, _)| id)
            }
            (true, ..) => None,
        };
        standing.insert(
            key.clone(),
            (age, (work.is_none(), work.unwrap_or_default())),
        );
    }
    Ok(standing)
}

/// Where a file that takes part sits: its group, its edition's key, and each
/// library with matching on that reaches its directory or a copy's.
struct Seat<'g> {
    group: &'g GroupKey,
    key: &'g str,
    libraries: BTreeSet<i64>,
}

/// The stored pairs that apply to `groups`, the groups held where they are,
/// and the files kept apart. A pair applies when V is at least [`BAR`], no
/// admin split it, both files take part, one library with matching on reaches
/// both, and their editions' keys differ: two files under one key are copies
/// under one label, which content matching does not settle.
///
/// # Errors
///
/// If the store refuses a read, or holds a pair this build does not read.
pub(super) fn applicable(
    conn: &Connection,
    groups: &BTreeMap<GroupKey, Group>,
) -> Result<Applicable, String> {
    let failed = |e: rusqlite::Error| format!("reading which matches apply: {e}");
    let _snapshot = crate::one_snapshot(conn).map_err(failed)?;
    // Of any fingerprint layout: the match pass scores only files it read
    // whole under its own, so a stored pair already says which layout.
    let whole: HashSet<String> = conn
        .prepare("SELECT sha256 FROM fingerprint WHERE outcome = 'whole'")
        .and_then(|mut stmt| stmt.query_map([], |r| r.get(0))?.collect())
        .map_err(failed)?;
    let seats = seats(groups, &matches::reach(conn)?, &whole);
    let (held, stands) = held(conn, groups)?;
    let mut found = Applicable {
        held,
        stands,
        ..Applicable::default()
    };
    for pair in matches::pairs(conn)? {
        if pair.state == State::Split {
            let (a, b) = (pair.new.sha_a, pair.new.sha_b);
            found.apart.entry(a.clone()).or_default().insert(b.clone());
            found.apart.entry(b).or_default().insert(a);
            continue;
        }
        if pair.new.v < BAR {
            continue;
        }
        let (Some(Some(a)), Some(Some(b))) = (
            seats.get(pair.new.sha_a.as_str()),
            seats.get(pair.new.sha_b.as_str()),
        ) else {
            continue;
        };
        if a.key == b.key || a.libraries.is_disjoint(&b.libraries) {
            continue;
        }
        let edge = Edge {
            pair: pair.id,
            v: pair.new.v,
            at: [
                (a.group.clone(), a.key.to_owned()),
                (b.group.clone(), b.key.to_owned()),
            ],
        };
        if a.group == b.group {
            found.together.push(edge);
        } else {
            found.edges.push(edge);
        }
    }
    Ok(found)
}

/// Each digest the groups hold, seated where it sits if it takes part (spec
/// §1): the only file of its edition, in a group with a volume, read whole.
/// The third condition, a library with matching on that reaches its
/// directory or a copy's, is the pair's to ask, since a pair needs one
/// library reaching both files. Bytes held in two places take no part: a
/// pair names a file by its digest, so it cannot say which of the two would
/// move.
fn seats<'g>(
    groups: &'g BTreeMap<GroupKey, Group>,
    reach: &HashMap<i64, BTreeSet<i64>>,
    whole: &HashSet<String>,
) -> HashMap<&'g str, Option<Seat<'g>>> {
    let mut seats: HashMap<&str, Option<Seat<'_>>> = HashMap::new();
    for (group_key, group) in groups {
        for (key, candidates) in &group.editions {
            for candidate in candidates {
                let libraries: BTreeSet<i64> = std::iter::once(candidate.root_id)
                    .chain(candidate.copies.iter().map(|copy| copy.root_id))
                    .filter_map(|root| reach.get(&root))
                    .flatten()
                    .copied()
                    .collect();
                let takes_part = group.volume.is_some()
                    && candidates.len() == 1
                    && whole.contains(&candidate.sha256);
                let seat = takes_part.then(|| Seat {
                    group: group_key,
                    key,
                    libraries,
                });
                seats
                    .entry(candidate.sha256.as_str())
                    .and_modify(|twice| *twice = None)
                    .or_insert(seat);
            }
        }
    }
    seats
}

/// Each group holding a file whose series a rule's literal `set_series`
/// names, or whose bytes sit in a work an admin gave a title, volume or
/// series by hand. Not where they sit there in an edition a join brought in:
/// that work is the host's, and hand values on the host never stop a join.
/// With [`Applicable::stands`], read from the series those works stand in.
fn held(
    conn: &Connection,
    groups: &BTreeMap<GroupKey, Group>,
) -> Result<(BTreeSet<GroupKey>, BTreeMap<GroupKey, String>), String> {
    let rows: Vec<(String, Option<String>)> = conn
        .prepare(
            "SELECT m.sha256, s.sort_title FROM member m
               JOIN edition e ON e.id = m.edition_id
               JOIN work w ON w.id = e.work_id
               LEFT JOIN series s ON s.id = w.series_id
              WHERE e.joined_by IS NULL
                AND (w.title_hand IS NOT NULL OR w.volume_hand IS NOT NULL
                     OR w.series_id_hand IS NOT NULL)",
        )
        .and_then(|mut stmt| {
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect()
        })
        .map_err(|e| format!("reading which works an admin set by hand: {e}"))?;
    let mut by_hand: HashMap<String, BTreeSet<Option<String>>> = HashMap::new();
    for (sha256, series) in rows {
        by_hand.entry(sha256).or_default().insert(series);
    }
    let mut held = BTreeSet::new();
    let mut stands = BTreeMap::new();
    for (key, group) in groups {
        let files = || group.editions.values().flatten();
        if !files()
            .any(|candidate| candidate.held_by_rule || by_hand.contains_key(&candidate.sha256))
        {
            continue;
        }
        held.insert(key.clone());
        if files().any(|candidate| candidate.held_by_rule) {
            continue;
        }
        let standing: BTreeSet<&Option<String>> = files()
            .filter_map(|candidate| by_hand.get(&candidate.sha256))
            .flatten()
            .collect();
        if let [Some(series)] = standing.into_iter().collect::<Vec<_>>()[..] {
            stands.insert(key.clone(), series.clone());
        }
    }
    Ok((held, stands))
}

#[cfg(test)]
mod tests;
