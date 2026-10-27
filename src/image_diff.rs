use crate::{
    git::{self, git},
    process::{self, Result},
};
use std::path::Path;
#[derive(Clone)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}
pub struct Comparison {
    pub before: Option<Frame>,
    pub after: Option<Frame>,
}
pub fn supported(path: &str) -> bool {
    Path::new(path).extension().is_some_and(|s| {
        ["png", "jpg", "jpeg", "gif", "webp", "bmp", "ico"]
            .iter()
            .any(|ext| s.eq_ignore_ascii_case(ext))
    })
}
pub fn decode(bytes: Vec<u8>) -> Result<Frame> {
    if bytes.len() > 16 * 1024 * 1024 {
        return Err(
            "This image exceeds the 16 MiB preview limit. Open it in an external editor.".into(),
        );
    }
    let dimensions = image::ImageReader::new(std::io::Cursor::new(&bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?
        .into_dimensions()
        .map_err(|e| e.to_string())?;
    if u64::from(dimensions.0) * u64::from(dimensions.1) > 8 * 1024 * 1024 {
        return Err(
            "This image exceeds the 8 megapixel preview limit. Open it in an external editor."
                .into(),
        );
    }
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(32 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader
        .decode()
        .map_err(|e| format!("Cannot preview this image: {e}. Use an external image editor."))?
        .to_rgba8();
    Ok(Frame {
        width: decoded.width(),
        height: decoded.height(),
        rgba: decoded.into_raw(),
    })
}
fn blob(root: &Path, id: &str) -> Result<Frame> {
    decode(process::git_bytes(
        root,
        &process::strings(&["cat-file", "blob", id]),
    )?)
}
fn index(root: &Path, path: &str) -> Result<Option<Frame>> {
    let record = git(root, &["ls-files", "--stage", "-z", "--", path])?;
    let Some((metadata, _)) = record.split_once('\t') else {
        return Ok(None);
    };
    let fields: Vec<_> = metadata.split_whitespace().collect();
    if fields.len() < 3 || fields[2] != "0" {
        return Err("Resolve this file's conflicts before comparing images.".into());
    }
    blob(root, fields[1]).map(Some)
}
fn head(root: &Path, path: &str) -> Result<Option<Frame>> {
    if git::resolve(root, "HEAD").is_err() {
        return Ok(None);
    }
    let record = git(root, &["ls-tree", "-z", "HEAD", "--", path])?;
    let Some((metadata, _)) = record.split_once('\t') else {
        return Ok(None);
    };
    let fields: Vec<_> = metadata.split_whitespace().collect();
    if fields.len() < 3 || fields[1] != "blob" {
        return Ok(None);
    }
    blob(root, fields[2]).map(Some)
}
pub fn compare(root: &Path, path: &str, staged: bool) -> Result<Comparison> {
    if !supported(path) {
        return Err("Choose a PNG, JPEG, GIF, WebP, BMP, or ICO file.".into());
    }
    let before = if staged {
        let changes =
            git::parse_status(&git(root, &["status", "--porcelain=v1", "-z", "--", path])?);
        head(
            root,
            changes
                .first()
                .and_then(|c| c.original.as_deref())
                .unwrap_or(path),
        )?
    } else {
        index(root, path)?
    };
    let after = if staged {
        index(root, path)?
    } else {
        let target = crate::recovery::checked_path(root, path)?;
        if target.exists() {
            if std::fs::metadata(&target).map_err(|e| e.to_string())?.len() > 16 * 1024 * 1024 {
                return Err(
                    "This image exceeds the 16 MiB preview limit. Open it in an external editor."
                        .into(),
                );
            }
            Some(decode(std::fs::read(target).map_err(|e| e.to_string())?)?)
        } else {
            None
        }
    };
    Ok(Comparison { before, after })
}
