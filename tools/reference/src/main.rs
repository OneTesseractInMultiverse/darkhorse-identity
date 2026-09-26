//! Deterministic source inventory. Run from the repository root; no services needed.
use darkhorse_reference::inventory::{self, Entry};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

const MAX_SOURCE_FILES: usize = 1024;
const MAX_SOURCE_ENTRIES: usize = 4096;
const MAX_SOURCE_DEPTH: usize = 32;
const MAX_SOURCE_FILE_BYTES: u64 = 1_048_576;
const MAX_TOTAL_SOURCE_BYTES: usize = 32 * 1024 * 1024;

fn files(
    root: &Path,
    found: &mut Vec<PathBuf>,
    visited: &mut usize,
    depth: usize,
) -> Result<(), String> {
    if depth > MAX_SOURCE_DEPTH {
        return Err("Source tree depth limit exceeded.".into());
    }
    for child in fs::read_dir(root).map_err(|_| "Cannot read source directory.")? {
        let child = child.map_err(|_| "Cannot read source entry.")?;
        *visited = visited.saturating_add(1);
        if *visited > MAX_SOURCE_ENTRIES {
            return Err("Source tree entry limit exceeded.".into());
        }
        let kind = child
            .file_type()
            .map_err(|_| "Cannot inspect source entry.")?;
        if kind.is_symlink() {
            return Err("Source links require explicit review.".into());
        }
        if kind.is_dir() {
            files(&child.path(), found, visited, depth + 1)?;
        } else if child.path().extension().is_some_and(|ext| ext == "rs") {
            found.push(child.path());
        }
        if found.len() > MAX_SOURCE_FILES {
            return Err("Source inventory exceeded its file bound.".into());
        }
    }
    Ok(())
}
fn source(path: &Path, remaining_bytes: usize) -> Result<String, String> {
    let metadata = fs::symlink_metadata(path).map_err(|_| "Cannot inspect source file.")?;
    if !metadata.is_file()
        || metadata.len() > MAX_SOURCE_FILE_BYTES
        || metadata.len() > remaining_bytes as u64
    {
        return Err("Source file byte limit exceeded.".into());
    }
    let file = fs::File::open(path).map_err(|_| "Cannot open source.")?;
    let mut contents = String::new();
    file.take(MAX_SOURCE_FILE_BYTES + 1)
        .read_to_string(&mut contents)
        .map_err(|_| "Cannot read UTF-8 source.")?;
    if contents.len() as u64 > MAX_SOURCE_FILE_BYTES || contents.len() > remaining_bytes {
        return Err("Source file byte limit exceeded.".into());
    }
    Ok(contents)
}
fn read() -> Result<Vec<Entry>, String> {
    let mut paths = Vec::new();
    let mut visited = 0;
    for path in ["crates/adapters/src", "apps/server/src"] {
        files(Path::new(path), &mut paths, &mut visited, 0)?;
    }
    paths.sort();
    let mut found = Vec::new();
    let mut source_bytes = 0usize;
    for path in paths {
        let remaining_bytes = MAX_TOTAL_SOURCE_BYTES.saturating_sub(source_bytes);
        let contents = source(&path, remaining_bytes)?;
        source_bytes =
            inventory::bounded_total(source_bytes, contents.len(), MAX_TOTAL_SOURCE_BYTES)
                .map_err(|_| "Total source byte limit exceeded.")?;
        let path = path.to_str().ok_or("Non-UTF-8 source path.")?;
        let entries = inventory::parse(path, &contents)
            .map_err(|error| format!("Unsupported source inventory: {path}: {error:?}"))?;
        inventory::bounded_total(found.len(), entries.len(), inventory::MAX_ENTRIES)
            .map_err(|_| "Route inventory entry limit exceeded.")?;
        found.extend(entries);
    }
    inventory::merge(found).map_err(|_| "Invalid aggregate route inventory.".into())
}
fn run() -> Result<(), String> {
    if std::env::args_os().count() != 1 {
        return Err("Run the inventory without arguments from the repository root.".into());
    }
    let output = inventory::document(read()?)?;
    let mut stdout = std::io::stdout().lock();
    stdout
        .write_all(&output)
        .and_then(|()| stdout.write_all(b"\n"))
        .map_err(|_| "Cannot write route inventory.".into())
}
fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            let _ = writeln!(&mut std::io::stderr().lock(), "{error}");
            std::process::ExitCode::FAILURE
        }
    }
}
