use crate::git::Commit;

#[derive(Clone, Debug)]
pub struct GraphRow {
    pub lane: usize,
    pub edges: Vec<(usize, usize)>,
    pub width: usize,
}

/// Keep parent lanes alive until their commit is visited; a merge can have several parents.
pub fn layout(commits: &[Commit]) -> Vec<GraphRow> {
    let mut lanes: Vec<String> = vec![];
    let mut rows = vec![];
    for commit in commits {
        let lane = lanes
            .iter()
            .position(|id| id == &commit.id)
            .unwrap_or_else(|| {
                lanes.push(commit.id.clone());
                lanes.len() - 1
            });
        let before = lanes.clone();
        lanes.remove(lane);
        for (offset, parent) in commit.parents.iter().enumerate() {
            if !lanes.contains(parent) {
                lanes.insert((lane + offset).min(lanes.len()), parent.clone());
            }
        }
        let mut edges = vec![];
        for (from, id) in before.iter().enumerate() {
            if from != lane
                && let Some(to) = lanes.iter().position(|n| n == id)
            {
                edges.push((from, to));
            }
        }
        for parent in &commit.parents {
            if let Some(to) = lanes.iter().position(|n| n == parent) {
                edges.push((lane, to));
            }
        }
        rows.push(GraphRow {
            lane,
            edges,
            width: before.len().max(lanes.len()),
        });
    }
    rows
}

pub fn ancestors(commits: &[Commit], tip: &str) -> Vec<Commit> {
    use std::collections::{HashMap, HashSet};
    let by_id: HashMap<_, _> = commits.iter().map(|c| (c.id.as_str(), c)).collect();
    let mut pending = vec![tip];
    let mut seen = HashSet::new();
    while let Some(id) = pending.pop() {
        if seen.insert(id)
            && let Some(commit) = by_id.get(id)
        {
            pending.extend(commit.parents.iter().map(String::as_str));
        }
    }
    commits
        .iter()
        .filter(|c| seen.contains(c.id.as_str()))
        .cloned()
        .collect()
}

#[derive(Clone, Debug)]
pub struct SnapshotGroup {
    pub parent: Option<String>,
    pub snapshots: Vec<crate::git::Snapshot>,
}

/// Captures are siblings under their real parent; save order does not imply ancestry.
pub fn snapshot_groups(snapshots: &[crate::git::Snapshot]) -> Vec<SnapshotGroup> {
    let mut groups: Vec<SnapshotGroup> = vec![];
    for snapshot in snapshots {
        let parent = snapshot.parents.first().cloned();
        if let Some(group) = groups.iter_mut().find(|g| g.parent == parent) {
            group.snapshots.push(snapshot.clone());
        } else {
            groups.push(SnapshotGroup {
                parent,
                snapshots: vec![snapshot.clone()],
            });
        }
    }
    groups
}
