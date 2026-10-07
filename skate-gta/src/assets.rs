//! Locates the player's converted Skate 3 files. skate3rust.exe converts the
//! owned disc (default.xex + data) once, into
//! `<skate3rust>\data\installations\<id>\assets`; this mod only reads them.
use skate_data::collections::Collections;
use std::path::{Path, PathBuf};

pub const COLLECTIONS: &str = "private/stock/skater-collections.json";

pub fn resolve(asset_root: Option<&Path>, skate3rust_dir: Option<&Path>) -> Result<PathBuf, String> {
    if let Some(root) = asset_root {
        return validate(root.to_path_buf());
    }
    let dir = skate3rust_dir.ok_or(
        "Set Skate3RustDir (folder with skate3rust.exe) or AssetRoot in SkateGTA.ini",
    )?;
    let base = dir.join("data");
    let marker = base.join("installation.json");
    let text = std::fs::read_to_string(&marker).map_err(|e| {
        format!(
            "{}: {e}. Run skate3rust.exe once and select your default.xex to convert your Skate 3 files",
            marker.display()
        )
    })?;
    let directory = installation_directory(&text)?;
    validate(base.join(directory).join("assets"))
}

fn validate(root: PathBuf) -> Result<PathBuf, String> {
    let collections = root.join(COLLECTIONS);
    if collections.is_file() {
        Ok(root)
    } else {
        Err(format!("Converted Skate 3 data not found: {}", collections.display()))
    }
}

/// `installations/<32 hex>` from installation.json, rejecting anything else.
fn installation_directory(text: &str) -> Result<PathBuf, String> {
    let text = text.trim_start_matches('\u{feff}');
    let version = json_field(text, "version");
    if version.as_deref() != Some("1") {
        return Err("Unsupported skate3rust installation.json version".into());
    }
    let directory = json_field(text, "directory").ok_or("installation.json has no directory")?;
    let directory = directory.replace('\\', "/");
    let mut parts = directory.split('/');
    let (Some("installations"), Some(id), None) = (parts.next(), parts.next(), parts.next()) else {
        return Err("Invalid installation directory".into());
    };
    if id.len() != 32 || !id.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
        return Err("Invalid installation directory".into());
    }
    Ok(PathBuf::from("installations").join(id))
}

/// Top-level scalar of a small flat JSON object, as text without quotes.
fn json_field(text: &str, name: &str) -> Option<String> {
    let key = format!("\"{name}\"");
    let rest = &text[text.find(&key)? + key.len()..];
    let rest = rest.trim_start().strip_prefix(':')?.trim_start();
    if let Some(quoted) = rest.strip_prefix('"') {
        let mut out = String::new();
        let mut chars = quoted.chars();
        while let Some(c) = chars.next() {
            match c {
                '"' => return Some(out),
                '\\' => out.push(chars.next()?),
                c => out.push(c),
            }
        }
        None
    } else {
        let end = rest.find([',', '}', '\n', '\r', ' ']).unwrap_or(rest.len());
        Some(rest[..end].to_string())
    }
}

pub fn load_collections(root: &Path) -> Result<Collections, String> {
    Collections::load(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_installation_marker() {
        let dir = installation_directory(
            "{\"version\": 1, \"directory\": \"installations\\\\0123456789abcdef0123456789abcdef\"}",
        )
        .unwrap();
        assert_eq!(dir, PathBuf::from("installations").join("0123456789abcdef0123456789abcdef"));
    }

    #[test]
    fn rejects_escaping_installation_paths() {
        for bad in ["../x", "installations/../../etc", "installations/ABC"] {
            let text = format!("{{\"version\":1,\"directory\":\"{bad}\"}}");
            assert!(installation_directory(&text).is_err(), "{bad}");
        }
        assert!(installation_directory("{\"version\":2,\"directory\":\"installations/x\"}").is_err());
    }

    #[test]
    fn missing_data_explains_the_conversion_step() {
        let error = resolve(None, Some(Path::new("/nonexistent/skate3rust"))).unwrap_err();
        assert!(error.contains("default.xex"), "{error}");
    }
}
