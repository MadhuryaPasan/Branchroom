use crate::process::Result;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};

/// Replace the complete file only after the new content has been written and flushed.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    replace_with(path, |file| {
        file.write_all(bytes).map_err(|e| e.to_string())
    })
}
pub fn copy_atomic(source: &Path, path: &Path) -> Result<()> {
    replace_with(path, |file| {
        let mut input = fs::File::open(source).map_err(|e| e.to_string())?;
        std::io::copy(&mut input, file).map_err(|e| e.to_string())?;
        Ok(())
    })
}
fn replace_with(path: &Path, write: impl FnOnce(&mut fs::File) -> Result<()>) -> Result<()> {
    let parent = path
        .parent()
        .ok_or("Choose a file with a parent directory.")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let temporary = parent.join(format!(".branchroom-{}.tmp", crate::snapshot::unique_id()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|e| e.to_string())?;
        write(&mut file)?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        fs::rename(&temporary, path).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}
