use crate::{
    git,
    process::{self, Result},
};
use std::{collections::BTreeSet, path::Path};

#[derive(Clone, Debug, PartialEq)]
pub enum LineKind {
    Context,
    Added,
    Removed,
    Meta,
}
#[derive(Clone, Debug)]
pub struct DiffLine {
    pub text: String,
    pub kind: LineKind,
    pub old: Option<usize>,
    pub new: Option<usize>,
}
#[derive(Clone, Debug)]
pub struct Hunk {
    pub header: String,
    pub old_start: usize,
    pub new_start: usize,
    pub lines: Vec<DiffLine>,
}
#[derive(Clone, Debug, Default)]
pub struct Patch {
    pub prefix: String,
    pub hunks: Vec<Hunk>,
    pub binary: bool,
    pub raw: String,
}

fn range(value: &str) -> Option<(usize, usize)> {
    let mut parts = value.trim_start_matches(['-', '+']).split(',');
    Some((
        parts.next()?.parse().ok()?,
        parts.next().unwrap_or("1").parse().ok()?,
    ))
}

// Keep replacement pairs together when skipped removals become context.
// Otherwise selecting the first pair in a large replacement appends its new line at the end.
fn selection_order(hunk: &Hunk) -> Vec<usize> {
    let mut ordered = Vec::new();
    let mut index = 0;
    while index < hunk.lines.len() {
        if matches!(hunk.lines[index].kind, LineKind::Added | LineKind::Removed) {
            let mut removed = Vec::new();
            let mut added = Vec::new();
            while index < hunk.lines.len()
                && matches!(hunk.lines[index].kind, LineKind::Added | LineKind::Removed)
            {
                let kind = &hunk.lines[index].kind;
                let mut unit = vec![index];
                index += 1;
                if index < hunk.lines.len() && hunk.lines[index].text.starts_with("\\ No newline") {
                    unit.push(index);
                    index += 1;
                }
                if *kind == LineKind::Removed {
                    removed.push(unit);
                } else {
                    added.push(unit);
                }
            }
            for pair in 0..removed.len().max(added.len()) {
                if let Some(unit) = removed.get(pair) {
                    ordered.extend(unit);
                }
                if let Some(unit) = added.get(pair) {
                    ordered.extend(unit);
                }
            }
        } else {
            ordered.push(index);
            index += 1;
        }
    }
    ordered
}

impl Patch {
    pub fn parse(raw: &str) -> Self {
        let mut patch = Self {
            raw: raw.into(),
            binary: raw.contains("Binary files ") || raw.contains("GIT binary patch"),
            ..Default::default()
        };
        let (mut old, mut new) = (0, 0);
        for line in raw.split_inclusive('\n') {
            let text = line.trim_end_matches('\n');
            if text.starts_with("@@ ") {
                let fields: Vec<_> = text.split_whitespace().collect();
                if let (Some((a, _)), Some((b, _))) = (
                    fields.get(1).and_then(|s| range(s)),
                    fields.get(2).and_then(|s| range(s)),
                ) {
                    old = a;
                    new = b;
                    patch.hunks.push(Hunk {
                        header: text.into(),
                        old_start: a,
                        new_start: b,
                        lines: vec![],
                    });
                    continue;
                }
            }
            if let Some(hunk) = patch.hunks.last_mut() {
                let (kind, a, b) = match text.as_bytes().first() {
                    Some(b'+') => (LineKind::Added, None, Some(new)),
                    Some(b'-') => (LineKind::Removed, Some(old), None),
                    Some(b' ') => (LineKind::Context, Some(old), Some(new)),
                    _ => (LineKind::Meta, None, None),
                };
                if a.is_some() {
                    old += 1;
                }
                if b.is_some() {
                    new += 1;
                }
                hunk.lines.push(DiffLine {
                    text: text.into(),
                    kind,
                    old: a,
                    new: b,
                });
            } else {
                patch.prefix.push_str(line);
            }
        }
        patch
    }

    /// Keep skipped removals as context and omit skipped additions, then recount the hunk.
    pub fn selection(
        &self,
        hunks: &BTreeSet<usize>,
        lines: &BTreeSet<(usize, usize)>,
    ) -> Result<String> {
        if self.binary || self.prefix.contains("rename from ") {
            return Err("Stage binary or renamed files as a whole.".into());
        }
        let mut prefix = self.prefix.clone();
        let all_removals_selected = self.hunks.iter().enumerate().all(|(hi, h)| {
            h.lines.iter().enumerate().all(|(li, l)| {
                l.kind != LineKind::Removed || hunks.contains(&hi) || lines.contains(&(hi, li))
            })
        });
        if prefix.contains("deleted file mode") && !all_removals_selected {
            let new_path = prefix
                .lines()
                .find_map(|l| l.strip_prefix("--- "))
                .ok_or("Missing file header.")?
                .replacen("a/", "b/", 1);
            prefix = prefix
                .lines()
                .filter(|l| !l.starts_with("deleted file mode") && !l.starts_with("index "))
                .map(|l| {
                    if l.starts_with("+++ ") {
                        format!("+++ {new_path}\n")
                    } else {
                        format!("{l}\n")
                    }
                })
                .collect();
        }
        let mut out = prefix;
        let mut offset: isize = 0;
        let mut changed = false;
        for (hi, hunk) in self.hunks.iter().enumerate() {
            let whole = hunks.contains(&hi);
            let mut body = String::new();
            let (mut old_count, mut new_count) = (0, 0);
            let mut used = false;
            let mut include_marker = false;
            for li in selection_order(hunk) {
                let line = &hunk.lines[li];
                let selected = whole || lines.contains(&(hi, li));
                match line.kind {
                    LineKind::Added => {
                        if selected {
                            body.push_str(&line.text);
                            body.push('\n');
                            new_count += 1;
                            used = true;
                            include_marker = true;
                        } else {
                            include_marker = false;
                        }
                    }
                    LineKind::Removed => {
                        old_count += 1;
                        if selected {
                            body.push_str(&line.text);
                            body.push('\n');
                            used = true;
                        } else {
                            body.push(' ');
                            body.push_str(&line.text[1..]);
                            body.push('\n');
                            new_count += 1;
                        }
                        include_marker = true;
                    }
                    LineKind::Context => {
                        body.push_str(&line.text);
                        body.push('\n');
                        old_count += 1;
                        new_count += 1;
                        include_marker = true;
                    }
                    LineKind::Meta => {
                        if include_marker && line.text.starts_with("\\ No newline") {
                            body.push_str(&line.text);
                            body.push('\n');
                        }
                    }
                }
            }
            if used {
                let new_start = (hunk.old_start as isize + offset - isize::from(new_count == 0))
                    .max(0) as usize;
                out.push_str(&format!(
                    "@@ -{},{} +{},{} @@\n{}",
                    hunk.old_start,
                    old_count,
                    if old_count == 0 {
                        new_start + 1
                    } else {
                        new_start
                    },
                    new_count,
                    body
                ));
                offset += new_count as isize - old_count as isize;
                changed = true;
            }
        }
        if !changed {
            return Err("Select at least one changed line or hunk.".into());
        }
        Ok(out)
    }

    pub fn unstaging_selection(
        &self,
        hunks: &BTreeSet<usize>,
        lines: &BTreeSet<(usize, usize)>,
    ) -> Result<String> {
        let mut reversed = self.clone();
        let old_path = self
            .prefix
            .lines()
            .find_map(|l| l.strip_prefix("--- "))
            .unwrap_or("/dev/null")
            .replacen("a/", "b/", 1);
        let new_path = self
            .prefix
            .lines()
            .find_map(|l| l.strip_prefix("+++ "))
            .unwrap_or("/dev/null")
            .replacen("b/", "a/", 1);
        reversed.prefix = self
            .prefix
            .lines()
            .filter(|l| !l.starts_with("index "))
            .map(|l| {
                if l.starts_with("--- ") {
                    format!("--- {new_path}\n")
                } else if l.starts_with("+++ ") {
                    format!("+++ {old_path}\n")
                } else if let Some(mode) = l.strip_prefix("new file mode ") {
                    format!("deleted file mode {mode}\n")
                } else if let Some(mode) = l.strip_prefix("deleted file mode ") {
                    format!("new file mode {mode}\n")
                } else {
                    format!("{l}\n")
                }
            })
            .collect();
        for hunk in &mut reversed.hunks {
            std::mem::swap(&mut hunk.old_start, &mut hunk.new_start);
            for line in &mut hunk.lines {
                std::mem::swap(&mut line.old, &mut line.new);
                match line.kind {
                    LineKind::Added => {
                        line.kind = LineKind::Removed;
                        line.text.replace_range(..1, "-");
                    }
                    LineKind::Removed => {
                        line.kind = LineKind::Added;
                        line.text.replace_range(..1, "+");
                    }
                    _ => {}
                }
            }
        }
        reversed.selection(hunks, lines)
    }
}

pub fn file_patch(
    root: &Path,
    path: &str,
    staged: bool,
    ignore_whitespace: bool,
    full_context: bool,
) -> Result<Patch> {
    let mut args = vec!["diff", "--no-color", "--no-ext-diff", "--no-textconv"];
    if staged {
        args.push("--cached");
    }
    if ignore_whitespace {
        args.push("--ignore-all-space");
    }
    if full_context {
        args.push("--unified=100000");
    }
    let status = git::parse_status(&git::git(
        root,
        &["status", "--porcelain=v1", "-z", "--", path],
    )?);
    let untracked = status.first().is_some_and(|c| c.index == '?');
    if untracked && !staged {
        args.push("--no-index");
        args.extend(["--", "/dev/null", path]);
    } else {
        args.extend(["--", path]);
    }
    git::git(root, &args).map(|text| Patch::parse(&text))
}

pub fn apply_to_index(root: &Path, patch: &str, reverse: bool) -> Result<String> {
    crate::safety::ensure_writable(root)?;
    let locks = crate::commit_lock::read(root)?;
    if !locks.is_empty() {
        let paths = process::input(
            "git",
            root,
            &process::strings(&["apply", "--numstat", "-z"]),
            patch.as_bytes(),
            &[],
        )?;
        if paths.split('\0').any(|record| {
            crate::commit_lock::is_locked(&locks, record.splitn(3, '\t').nth(2).unwrap_or(record))
        }) {
            return Err("This patch includes a file excluded from commits. Unlock the file before staging its lines.".into());
        }
    }
    let mut args = process::strings(&["apply", "--cached", "--recount", "--whitespace=nowarn"]);
    if reverse {
        args.push("--reverse".into());
    }
    process::input("git", root, &args, patch.as_bytes(), &[])
}
