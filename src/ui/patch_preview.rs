use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RowKind {
    File,
    Hunk,
    Added,
    Removed,
    Context,
    Meta,
}

#[derive(Debug)]
struct Row<'a> {
    text: &'a str,
    kind: RowKind,
    old: Option<usize>,
    new: Option<usize>,
}

fn rows(text: &str) -> Vec<Row<'_>> {
    let mut old = 0;
    let mut new = 0;
    let mut in_hunk = false;
    text.lines()
        .map(|line| {
            let mut row = Row {
                text: line,
                kind: RowKind::Meta,
                old: None,
                new: None,
            };
            if line.starts_with("diff --git ") {
                row.kind = RowKind::File;
                in_hunk = false;
            } else if line.starts_with("@@ ") {
                let mut parts = line.split_whitespace();
                let _ = parts.next();
                let number = |value: Option<&str>| {
                    value
                        .and_then(|s| s.get(1..))
                        .and_then(|s| s.split(',').next())
                        .and_then(|s| s.parse::<usize>().ok())
                };
                if let (Some(a), Some(b)) = (number(parts.next()), number(parts.next())) {
                    old = a;
                    new = b;
                    in_hunk = true;
                    row.kind = RowKind::Hunk;
                } else {
                    in_hunk = false;
                }
            } else if in_hunk {
                match line.as_bytes().first() {
                    Some(b'+') => {
                        row.kind = RowKind::Added;
                        row.new = Some(new);
                        new += 1;
                    }
                    Some(b'-') => {
                        row.kind = RowKind::Removed;
                        row.old = Some(old);
                        old += 1;
                    }
                    Some(b' ') => {
                        row.kind = RowKind::Context;
                        row.old = Some(old);
                        row.new = Some(new);
                        old += 1;
                        new += 1;
                    }
                    _ => {}
                }
            }
            row
        })
        .collect()
}

fn tint(base: Color32, color: Color32) -> Color32 {
    let mix = |a: u8, b: u8| ((a as u16 * 11 + b as u16) / 12) as u8;
    Color32::from_rgb(
        mix(base.r(), color.r()),
        mix(base.g(), color.g()),
        mix(base.b(), color.b()),
    )
}

/// A line-oriented view: the full, unmodified patch remains available to copy.
pub(super) fn summary(text: &str) -> Option<&str> {
    text.lines()
        .find(|line| line.contains("file changed,") || line.contains("files changed,"))
        .map(str::trim)
}

pub(super) fn show(ui: &mut egui::Ui, text: &str, raw: bool) {
    if text.is_empty() {
        subtitle(ui, "No changes to show.");
        return;
    }
    let all_rows = rows(text);
    let has_hunks = all_rows.iter().any(|r| r.kind == RowKind::Hunk);
    let rows: Vec<_> = all_rows
        .into_iter()
        .filter(|row| {
            raw || !has_hunks
                || row.kind != RowKind::Meta
                || row.text.starts_with("\\ No newline")
                || row.text.starts_with("Binary files ")
                || row.text == "GIT binary patch"
        })
        .collect();
    // A conservative monospaced width keeps long lines horizontally scrollable.
    let max_chars = rows
        .iter()
        .map(|r| {
            r.text
                .chars()
                .map(|c| if c == '\t' { 4 } else { 1 })
                .sum::<usize>()
        })
        .max()
        .unwrap_or(0);
    let width = (ui.available_width() - 16.).max(92. + max_chars as f32 * 8.);
    egui::ScrollArea::both()
        .id_salt("patch_preview_scroll")
        .auto_shrink([false, false])
        .show_rows(ui, 24., rows.len(), |ui, visible| {
            ui.set_min_width(width);
            ui.spacing_mut().item_spacing = egui::vec2(0., 0.);
            ui.spacing_mut().interact_size.y = 24.;
            for row in &rows[visible] {
                let (fill, color) = match row.kind {
                    RowKind::File => (RAISED(), INK()),
                    RowKind::Hunk => (SELECTED(), ACCENT()),
                    RowKind::Added => (tint(SURFACE(), SUCCESS()), SUCCESS()),
                    RowKind::Removed => (tint(SURFACE(), DANGER()), DANGER()),
                    RowKind::Meta => (SURFACE(), MUTED()),
                    RowKind::Context => (SURFACE(), INK()),
                };
                egui::Frame::new().fill(fill).show(ui, |ui| {
                    ui.set_min_width(width);
                    ui.set_height(24.);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.;
                        let gutter = format!(
                            "{:>4}  {:>4}",
                            row.old.map(|n| n.to_string()).unwrap_or_default(),
                            row.new.map(|n| n.to_string()).unwrap_or_default()
                        );
                        ui.add_sized(
                            [78., 24.],
                            egui::Label::new(
                                RichText::new(gutter).monospace().size(12.).color(MUTED()),
                            ),
                        );
                        ui.add_space(8.);
                        let display = if !raw && row.kind == RowKind::File {
                            row.text
                                .strip_prefix("diff --git a/")
                                .and_then(|line| line.split_once(" b/"))
                                .map(|(_, destination)| destination)
                                .unwrap_or(row.text)
                        } else {
                            row.text
                        };
                        let mut content = RichText::new(display).monospace().size(13.).color(color);
                        if row.kind == RowKind::File {
                            content = content.strong();
                        }
                        ui.add(egui::Label::new(content).selectable(true).extend());
                    });
                });
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn line_numbers_follow_hunks_and_restart_for_each_file() {
        let lines = rows(
            "diff --git a/a b/a\n--- a/a\n+++ b/a\n@@ -4,2 +4,2 @@\n same\n-old\n+new\n\\ No newline at end of file\ndiff --git a/b b/b\n@@ -1 +1 @@\n-before\n+after\n",
        );
        assert_eq!((lines[4].old, lines[4].new), (Some(4), Some(4)));
        assert_eq!(
            (lines[5].kind, lines[5].old, lines[5].new),
            (RowKind::Removed, Some(5), None)
        );
        assert_eq!(
            (lines[6].kind, lines[6].old, lines[6].new),
            (RowKind::Added, None, Some(5))
        );
        assert_eq!(lines[7].kind, RowKind::Meta);
        assert_eq!((lines[10].old, lines[11].new), (Some(1), Some(1)));
    }
}
