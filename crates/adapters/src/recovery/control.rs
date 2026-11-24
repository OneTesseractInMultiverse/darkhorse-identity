use super::{bounded, proof};
use darkhorse_application::recovery::Denied;
use nix::{
    dir::Dir,
    fcntl::{OFlag, openat},
    sys::stat::Mode,
};
use std::{
    fs::{File, OpenOptions},
    io,
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::Path,
};

pub(super) fn directory(path: &Path) -> Result<Option<File>, Denied> {
    let flags = OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW;
    let opened = OpenOptions::new()
        .read(true)
        .custom_flags(flags.bits())
        .open(path);
    match opened {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Ok(dir) if dir.metadata().map_err(|_| Denied)?.permissions().mode() & 0o7777 == 0o755 => {
            Ok(Some(dir))
        }
        _ => Err(Denied),
    }
}

fn regular(dir: &File, name: &str, max: u64) -> Result<Vec<u8>, Denied> {
    // Nonblocking open lets fstat reject a FIFO even when it has no writer.
    let flags = OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC;
    let file = File::from(openat(dir, name, flags, Mode::empty()).map_err(|_| Denied)?);
    let metadata = file.metadata().map_err(|_| Denied)?;
    if !metadata.is_file()
        || metadata.mode() & 0o222 != 0
        || metadata.nlink() != 1
        || metadata.len() > max
    {
        return Err(Denied);
    }
    bounded(file, max)
}

pub(super) struct Control {
    pub(super) deployment: String,
    pub(super) administrator: Option<[u8; 32]>,
    pub(super) serving: bool,
    pub(super) receipt: Option<String>,
}

impl Control {
    // All reads and the eventual rename address the same locked directory inode.
    pub(super) fn read(dir: &File, issuer: &str) -> Result<Self, Denied> {
        if !proof::canonical_origin(issuer) {
            return Err(Denied);
        }
        let entries = Dir::openat(
            dir,
            ".",
            OFlag::O_DIRECTORY | OFlag::O_CLOEXEC,
            Mode::empty(),
        )
        .map_err(|_| Denied)?;
        let mut serving = false;
        let mut administrator = None;
        let mut receipt = None;
        for entry in entries {
            let entry = entry.map_err(|_| Denied)?;
            let name = entry.file_name().to_str().map_err(|_| Denied)?;
            match name {
                "." | ".." | "deployment" => {}
                "serving" => serving = true,
                "administrator" => {
                    administrator = Some(regular(dir, name, 32)?.try_into().map_err(|_| Denied)?)
                }
                name if name.starts_with("quarantine-") && receipt.is_none() => {
                    receipt = Some(name.to_owned())
                }
                _ => return Err(Denied),
            }
        }
        let max_binding = (36 + 2 + proof::MAX_ISSUER) as u64;
        let bytes = regular(dir, "deployment", max_binding)?;
        let binding = std::str::from_utf8(&bytes).map_err(|_| Denied)?;
        let deployment = binding
            .strip_suffix(&format!("\n{issuer}\n"))
            .ok_or(Denied)?;
        if !proof::canonical_uuid(deployment) {
            return Err(Denied);
        }
        for name in serving
            .then_some("serving")
            .into_iter()
            .chain(receipt.as_deref())
        {
            if regular(dir, name, max_binding)? != bytes {
                return Err(Denied);
            }
        }
        Ok(Self {
            deployment: deployment.to_owned(),
            administrator,
            serving,
            receipt,
        })
    }
}
