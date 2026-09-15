//! Small shared CLI helpers; the server's conversion API remains independent.

mod finite;

use std::path::{Component, Path, PathBuf};

use serde::Serialize;

/// Discover regular files without following directory or file symlinks.
pub fn discover(path: &Path) -> Result<(bool, Vec<PathBuf>), String> {
    let metadata =
        std::fs::symlink_metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if metadata.is_file() {
        return Ok((false, vec![path.to_owned()]));
    }
    if !metadata.is_dir() {
        return Err(format!(
            "not a regular file or directory: {}",
            path.display()
        ));
    }
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(path).follow_links(false) {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.file_type().is_file()
            && entry
                .path()
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("ulg"))
        {
            files.push(entry.into_path());
        }
    }
    files.sort();
    if files.is_empty() {
        return Err(format!("No .ulg files found in {}", path.display()));
    }
    Ok((true, files))
}

/// Use a bounded pool scoped to this invocation, never Rayon global state.
pub fn worker_pool(jobs: Option<usize>) -> Result<rayon::ThreadPool, String> {
    let jobs = jobs.unwrap_or_else(|| {
        std::thread::available_parallelism()
            .map_or(1, usize::from)
            .min(256)
    });
    if !(1..=256).contains(&jobs) {
        return Err("--jobs must be between 1 and 256".into());
    }
    rayon::ThreadPoolBuilder::new()
        .num_threads(jobs)
        .build()
        .map_err(|e| e.to_string())
}

/// Reject nonfinite numbers rather than serde_json's implicit conversion to null.
/// Value's sorted object keys also make machine reports reproducible.
pub fn json<T: Serialize + ?Sized>(value: &T, pretty: bool) -> Result<String, String> {
    value.serialize(finite::Finite).map_err(|e| e.to_string())?;
    let value = serde_json::to_value(value).map_err(|e| e.to_string())?;
    if pretty {
        serde_json::to_string_pretty(&value)
    } else {
        serde_json::to_string(&value)
    }
    .map_err(|e| e.to_string())
}

pub fn write_json<T: Serialize + ?Sized>(
    path: &Path,
    value: &T,
    pretty: bool,
) -> Result<(), String> {
    let contents = json(value, pretty)?;
    std::fs::write(path, contents).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn analyzed_metadata_with_analyzers(
    path: &str,
    analyzers: Vec<Box<dyn crate::diagnostics::Analyzer>>,
) -> Result<crate::metadata::FlightMetadata, String> {
    let mut metadata = crate::metadata::extract_metadata(path).map_err(|e| e.to_string())?;
    if !metadata
        .topics
        .values()
        .any(|topic| topic.message_count > 0)
    {
        return Err("No data in ULog file".into());
    }
    metadata.analysis = Some(
        crate::analysis::analyze_with_analyzers(path, &metadata, analyzers)
            .map_err(|e| e.to_string())?,
    );
    Ok(metadata)
}

/// Escape every component, including uppercase bytes, for collision-free exports
/// even when source and destination filesystems differ in case sensitivity.
/// The root `index.json` component is reserved for the dataset index.
pub fn export_relative(path: &Path) -> Result<PathBuf, String> {
    let mut result = PathBuf::new();
    for component in path.components() {
        let Component::Normal(component) = component else {
            return Err(format!("invalid relative input path: {}", path.display()));
        };
        let text = component.to_str().ok_or("input path is not valid UTF-8")?;
        let mut escaped = String::new();
        for (index, byte) in text.bytes().enumerate() {
            let reserved = result.as_os_str().is_empty() && text == "index.json" && index == 0;
            if !reserved
                && (byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"-_.".contains(&byte))
            {
                escaped.push(byte as char);
            } else {
                use std::fmt::Write;
                write!(escaped, "%{byte:02X}").map_err(|e| e.to_string())?;
            }
        }
        result.push(escaped);
    }
    Ok(result)
}

/// Refuse reuse of nonempty output directories, source overlap, and symlink
/// ancestors. Canonical exports never silently overwrite a previous dataset.
pub fn ensure_output_safe(input: &Path, output: &Path) -> Result<(), String> {
    let input = input.canonicalize().map_err(|e| e.to_string())?;
    let absolute = if output.is_absolute() {
        output.to_owned()
    } else {
        std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(output)
    };
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::ParentDir => {
                normalized.pop();
            }
            Component::CurDir => {}
            component => normalized.push(component.as_os_str()),
        }
        match std::fs::symlink_metadata(&normalized) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(format!(
                    "output traverses symlink: {}",
                    normalized.display()
                ));
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("{}: {e}", normalized.display())),
        }
    }
    if input.starts_with(&normalized) || normalized.starts_with(&input) {
        return Err("output must not overlap the input file or directory".into());
    }
    match std::fs::read_dir(&normalized) {
        Ok(mut entries) => {
            if let Some(entry) = entries.next() {
                entry.map_err(|e| e.to_string())?;
                return Err(format!(
                    "output directory is not empty: {}",
                    output.display()
                ));
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("{}: {e}", output.display())),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_nonfinite_nested_values() {
        #[derive(Serialize)]
        struct Nested {
            values: Vec<Option<f64>>,
        }
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(json(
                &Nested {
                    values: vec![Some(value)]
                },
                false
            )
            .is_err());
        }
        assert_eq!(
            json(
                &Nested {
                    values: vec![None, Some(1.0)]
                },
                false
            )
            .unwrap(),
            r#"{"values":[null,1.0]}"#
        );
    }

    #[test]
    fn exported_paths_do_not_fold_case_or_drop_extensions() {
        assert_eq!(
            export_relative(Path::new("nested/a.ulg")).unwrap(),
            Path::new("nested/a.ulg")
        );
        assert_eq!(
            export_relative(Path::new("nested/a.ULG")).unwrap(),
            Path::new("nested/a.%55%4C%47")
        );
        assert_eq!(
            export_relative(Path::new("nested/a.%55")).unwrap(),
            Path::new("nested/a.%2555")
        );
    }
}
