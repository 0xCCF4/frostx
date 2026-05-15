/// Reading `frostx.toml` and companion TOML fragments directly from a
/// compressed tar archive without extracting files to disk.
///
/// All `.toml` entries found in the archive are collected into a
/// [`HashMap`] keyed by their path relative to the top-level project
/// directory (i.e. the first path component — the project folder name —
/// is stripped).  The values are the raw UTF-8 text of each file.
///
/// This map is then used by `config::load_from_archive` and
/// `include::resolve_includes_from_archive` so that include resolution
/// works for files that are contained within the archive.
use crate::config::project::Compression;
use crate::error::FrostxError;
use std::collections::HashMap;
use std::io::Read as _;
use std::path::Path;

/// Detect the compression format of a tar archive by inspecting its
/// leading magic bytes.
///
/// Recognised signatures:
/// - `1F 8B` → gzip
/// - `28 B5 2F FD` → zstandard
/// - `FD 37 7A 58 5A 00` → xz
///
/// # Errors
///
/// Returns an error if the file cannot be opened, read, or does not
/// start with a recognised magic sequence.
pub fn detect_compression(path: &Path) -> Result<Compression, FrostxError> {
    let mut file = std::fs::File::open(path)?;
    let mut magic = [0u8; 6];
    let n = file.read(&mut magic)?;

    if n >= 2 && magic[0] == 0x1f && magic[1] == 0x8b {
        Ok(Compression::Gz)
    } else if n >= 4 && magic[..4] == [0x28, 0xb5, 0x2f, 0xfd] {
        Ok(Compression::Zstd)
    } else if n >= 6 && magic[..6] == [0xfd, 0x37, 0x7a, 0x58, 0x5a, 0x00] {
        Ok(Compression::Xz)
    } else {
        Err(FrostxError::Config(format!(
            "{}: unrecognized archive format (expected a gzip, zstd, or xz tar archive)",
            path.display()
        )))
    }
}

/// Stream through a compressed tar archive and collect every `.toml`
/// entry whose content can be decoded as UTF-8.
///
/// The returned map is keyed by each file's path **relative to the
/// top-level project directory** (the first path component is stripped,
/// mirroring how [`crate::actions::archive::TarGz`] creates archives with
/// `tar::Builder::append_dir_all`).
///
/// Entries that cannot be read or are not valid UTF-8 are silently
/// skipped; they will surface as "not found" errors during include
/// resolution.
///
/// # Errors
///
/// Returns an error if the archive file cannot be opened or the
/// underlying decompressed stream cannot be initialised.
pub fn read_toml_entries(archive_path: &Path) -> Result<HashMap<String, String>, FrostxError> {
    let compression = detect_compression(archive_path)?;
    let file = std::fs::File::open(archive_path)?;
    match compression {
        Compression::Gz => {
            let dec = flate2::read::GzDecoder::new(file);
            collect_toml_entries(tar::Archive::new(dec))
        }
        Compression::Zstd => {
            let dec = zstd::Decoder::new(file).map_err(FrostxError::Io)?;
            collect_toml_entries(tar::Archive::new(dec))
        }
        Compression::Xz => {
            let dec = xz2::read::XzDecoder::new(file);
            collect_toml_entries(tar::Archive::new(dec))
        }
    }
}

fn collect_toml_entries<R: std::io::Read>(
    mut archive: tar::Archive<R>,
) -> Result<HashMap<String, String>, FrostxError> {
    let mut map = HashMap::new();

    let entries = archive.entries().map_err(FrostxError::Io)?;
    for entry in entries {
        let Ok(mut entry) = entry else { continue };

        let path = match entry.path() {
            Ok(p) => p.into_owned(),
            Err(_) => continue,
        };

        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }

        // Strip the top-level project directory that append_dir_all inserts.
        let mut components = path.components();
        components.next(); // skip project-dir component
        let rel: std::path::PathBuf = components.collect();
        if rel.as_os_str().is_empty() {
            continue;
        }

        let mut content = String::new();
        if entry.read_to_string(&mut content).is_ok() {
            map.insert(rel.to_string_lossy().into_owned(), content);
        }
        // Non-UTF-8 or unreadable entries are skipped; include resolution
        // will report them as "not found in archive".
    }

    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    /// Create a gzip tar archive using the same `append_dir_all` approach as
    /// production code.  `files` is a list of `(relative_path, content)` pairs
    /// written under a `project/` top-level directory.
    fn write_gz_archive(dir: &std::path::Path, files: &[(&str, &[u8])]) -> std::path::PathBuf {
        let src = tempdir().unwrap();
        for (rel, data) in files {
            let dest = src.path().join(rel);
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(dest, data).unwrap();
        }

        let archive_path = dir.join("project.tar.gz");
        let file = std::fs::File::create(&archive_path).unwrap();
        let enc = flate2::write::GzEncoder::new(file, flate2::Compression::best());
        let mut builder = tar::Builder::new(enc);
        builder.follow_symlinks(false);
        builder.append_dir_all("project", src.path()).unwrap();
        let enc = builder.into_inner().unwrap();
        enc.finish().unwrap();
        archive_path
    }

    #[test]
    fn detect_gz_by_magic() {
        let tmp = tempdir().unwrap();
        let archive = write_gz_archive(tmp.path(), &[("frostx.toml", b"id = \"test\"")]);
        let c = detect_compression(&archive).unwrap();
        assert!(matches!(c, Compression::Gz));
    }

    #[test]
    fn detect_unknown_format_errors() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("not_an_archive.tar.gz");
        std::fs::write(&path, b"just plain text").unwrap();
        assert!(detect_compression(&path).is_err());
    }

    #[test]
    fn collect_toml_entries_gz() {
        let tmp = tempdir().unwrap();
        let archive = write_gz_archive(tmp.path(), &[("frostx.toml", b"id = \"abc\"")]);
        let map = read_toml_entries(&archive).unwrap();
        assert!(map.contains_key("frostx.toml"), "map = {map:?}");
        assert!(map["frostx.toml"].contains("abc"));
    }

    #[test]
    fn non_toml_entries_are_ignored() {
        let tmp = tempdir().unwrap();
        let archive = write_gz_archive(
            tmp.path(),
            &[
                ("frostx.toml", b"id = \"x\""),
                ("src/main.rs", b"fn main() {}"),
            ],
        );
        let map = read_toml_entries(&archive).unwrap();
        assert_eq!(map.len(), 1, "only the .toml file should be in the map");
        assert!(map.contains_key("frostx.toml"));
    }

    #[test]
    fn relative_include_toml_in_archive() {
        let main_toml = b"id = \"abc\"\ninclude = [\"./extra.toml\"]\n";
        let extra_toml = b"[[rule]]\nafter = \"90d\"\nactions = []\n";

        let tmp = tempdir().unwrap();
        let archive = write_gz_archive(
            tmp.path(),
            &[("frostx.toml", main_toml), ("extra.toml", extra_toml)],
        );

        let map = read_toml_entries(&archive).unwrap();
        assert!(map.contains_key("frostx.toml"), "map = {map:?}");
        assert!(map.contains_key("extra.toml"), "map = {map:?}");
    }
}
