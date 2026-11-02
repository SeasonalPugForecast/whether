//! Comment-preserving edits of a configuration file (`whether locate --add`, the GUI's
//! location manager and settings). Everything outside the edited keys keeps its
//! bytes.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use toml_edit::{ArrayOfTables, DocumentMut, Item, Table, value};

use crate::types::LocationConfig;

/// The comment written above added locations.
const ADDED_COMMENT: &str = "# added by whether locate";

/// Why a configuration file could not be edited.
#[derive(Debug, thiserror::Error)]
pub enum EditError {
    /// The file could not be read or written.
    #[error("{}: {source}", path.display())]
    Io {
        /// The file.
        path: PathBuf,
        /// The I/O error.
        source: std::io::Error,
    },
    /// The file is not valid TOML.
    #[error("{}: {message}", path.display())]
    Parse {
        /// The file.
        path: PathBuf,
        /// The parser's message.
        message: String,
    },
    /// `location` is not an array of tables.
    #[error("`location` in {} is not a list of [[location]] tables", .0.display())]
    Shape(PathBuf),
    /// A location with this name already exists.
    #[error("a location named `{0}` already exists")]
    Duplicate(String),
    /// No location has this name.
    #[error("no location named `{0}`")]
    NotFound(String),
    /// The location to add has no name.
    #[error("the location has no name")]
    Unnamed,
    /// A key on the way to the one to set is a value, not a table, or the value
    /// cannot be written.
    #[error("cannot set `{0}`")]
    Key(String),
}

/// Sets each dotted key (`general.units`, `display.gui.sections`) to its value in one
/// write. A replaced value keeps its comment; missing tables are created (`[a.b]`
/// headers, with `[a]` left implicit). Returns the file's previous text, for
/// [`replace`] to restore (empty when the file did not exist).
pub fn set_values(path: &Path, values: &[(String, toml::Value)]) -> Result<String, EditError> {
    let previous = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(source) => {
            return Err(EditError::Io {
                path: path.to_path_buf(),
                source,
            });
        }
    };
    let mut doc = read(path, true)?;
    for (key, v) in values {
        set_key(&mut doc, key, v).ok_or_else(|| EditError::Key(key.clone()))?;
    }
    write_atomic(path, &doc.to_string())?;
    Ok(previous)
}

/// Replaces the file's text atomically (restoring what [`set_values`] returned).
pub fn replace(path: &Path, text: &str) -> Result<(), EditError> {
    write_atomic(path, text)
}

/// Sets `key` (dotted) in `doc`; `None` when a part is not a table or the value
/// has no TOML form here.
fn set_key(doc: &mut DocumentMut, key: &str, v: &toml::Value) -> Option<()> {
    let new = edit_value(v)?;
    let mut parts: Vec<&str> = key.split('.').collect();
    let leaf = parts.pop()?;
    let mut table = doc.as_table_mut() as &mut dyn toml_edit::TableLike;
    for (depth, part) in parts.iter().enumerate() {
        if table.get(part).is_none() {
            let mut t = Table::new();
            // Only the innermost table gets a header; outer ones stay implicit.
            t.set_implicit(depth + 1 < parts.len());
            table.insert(part, Item::Table(t));
        }
        table = table.get_mut(part)?.as_table_like_mut()?;
    }
    match table.get_mut(leaf).and_then(Item::as_value_mut) {
        Some(old) => {
            let decor = old.decor().clone();
            *old = new;
            *old.decor_mut() = decor;
        }
        None => {
            table.insert(leaf, Item::Value(new));
        }
    }
    Some(())
}

/// A `toml::Value` as an editable value (dates and tables are not needed here).
fn edit_value(v: &toml::Value) -> Option<toml_edit::Value> {
    Some(match v {
        toml::Value::String(s) => s.as_str().into(),
        toml::Value::Integer(i) => (*i).into(),
        toml::Value::Float(f) => (*f).into(),
        toml::Value::Boolean(b) => (*b).into(),
        toml::Value::Array(items) => {
            let mut array = toml_edit::Array::new();
            for item in items {
                array.push(edit_value(item)?);
            }
            toml_edit::Value::Array(array)
        }
        toml::Value::Datetime(_) | toml::Value::Table(_) => return None,
    })
}

/// Appends `loc` as a `[[location]]` table after the last existing one, with a
/// `# added by whether locate` comment. A missing file is created.
pub fn add_location(path: &Path, loc: &LocationConfig) -> Result<(), EditError> {
    let name = loc.name.as_deref().ok_or(EditError::Unnamed)?;
    let mut doc = read(path, true)?;
    let locations = locations_mut(&mut doc, path)?;
    if locations.iter().any(|t| table_name(t) == Some(name)) {
        return Err(EditError::Duplicate(name.to_string()));
    }
    let mut table = location_table(loc);
    let prefix = if locations.is_empty() && doc_is_empty(path) {
        format!("{ADDED_COMMENT}\n")
    } else {
        format!("\n{ADDED_COMMENT}\n")
    };
    table.decor_mut().set_prefix(prefix);
    locations_mut(&mut doc, path)?.push(table);
    write_atomic(path, &doc.to_string())
}

/// Removes the `[[location]]` named `name` (and the comments directly above it).
pub fn remove_location(path: &Path, name: &str) -> Result<(), EditError> {
    let mut doc = read(path, false)?;
    let locations = locations_mut(&mut doc, path)?;
    let index = locations
        .iter()
        .position(|t| table_name(t) == Some(name))
        .ok_or_else(|| EditError::NotFound(name.to_string()))?;
    locations.remove(index);
    write_atomic(path, &doc.to_string())
}

fn read(path: &Path, missing_ok: bool) -> Result<DocumentMut, EditError> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if missing_ok && e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(source) => {
            return Err(EditError::Io {
                path: path.to_path_buf(),
                source,
            });
        }
    };
    text.parse()
        .map_err(|e: toml_edit::TomlError| EditError::Parse {
            path: path.to_path_buf(),
            message: e.message().to_string(),
        })
}

fn doc_is_empty(path: &Path) -> bool {
    fs::metadata(path).map_or(true, |m| m.len() == 0)
}

/// The document's `[[location]]` tables, created when absent.
fn locations_mut<'d>(
    doc: &'d mut DocumentMut,
    path: &Path,
) -> Result<&'d mut ArrayOfTables, EditError> {
    let item = doc
        .entry("location")
        .or_insert_with(|| Item::ArrayOfTables(ArrayOfTables::new()));
    item.as_array_of_tables_mut()
        .ok_or_else(|| EditError::Shape(path.to_path_buf()))
}

fn table_name(table: &Table) -> Option<&str> {
    table.get("name").and_then(Item::as_str)
}

/// Floats as written by a person: `f32` elevations keep their short form.
fn short_f64(v: f32) -> f64 {
    v.to_string().parse().unwrap_or(f64::from(v))
}

/// A `[[location]]` table with the set keys in the documented order.
fn location_table(loc: &LocationConfig) -> Table {
    let mut t = Table::new();
    let strings = [
        ("name", &loc.name),
        ("zip", &loc.zip),
        ("country", &loc.country),
        ("query", &loc.query),
    ];
    for (key, v) in strings {
        if let Some(v) = v {
            t.insert(key, value(v.as_str()));
        }
    }
    if let Some(lat) = loc.latitude {
        t.insert("latitude", value(lat));
    }
    if let Some(lon) = loc.longitude {
        t.insert("longitude", value(lon));
    }
    if let Some(tz) = &loc.timezone {
        t.insert("timezone", value(tz.as_str()));
    }
    if let Some(offset) = &loc.utc_offset {
        t.insert("utc_offset", value(offset.as_str()));
    }
    if let Some(provider) = loc.provider {
        t.insert("provider", value(provider.id()));
    }
    if let Some(elevation) = loc.elevation {
        t.insert("elevation", value(short_f64(elevation)));
    }
    t
}

/// Writes `text` to a temporary file next to `path` and renames it over `path`, so
/// readers see the old or the new file, never a partial one. The original's
/// permissions are kept.
fn write_atomic(path: &Path, text: &str) -> Result<(), EditError> {
    let io = |source| EditError::Io {
        path: path.to_path_buf(),
        source,
    };
    let dir = path
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(dir).map_err(io)?;
    let file_name = path
        .file_name()
        .map_or_else(|| "config.toml".into(), |n| n.to_string_lossy());
    let tmp = dir.join(format!(".{file_name}.{}.tmp", std::process::id()));
    let result = (|| {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        if let Ok(meta) = fs::metadata(path) {
            fs::set_permissions(&tmp, meta.permissions())?;
        }
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result.map_err(io)
}

#[cfg(test)]
mod tests {
    use super::*;
    use whether_core::model::ProviderId;

    #[test]
    fn set_values_keeps_comments_and_creates_tables() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, COMMENTED).unwrap();
        let values = vec![
            ("general.refresh".to_string(), toml::Value::from("30m")),
            ("general.units".to_string(), toml::Value::from("imperial")),
            (
                "display.gui.sections".to_string(),
                toml::Value::Array(vec!["current".into(), "daily".into()]),
            ),
        ];
        let previous = set_values(&path, &values).unwrap();
        assert_eq!(previous, COMMENTED);
        let text = fs::read_to_string(&path).unwrap();
        assert!(
            text.contains("refresh = \"30m\"   # keep this comment"),
            "the inline comment stays: {text}"
        );
        assert!(text.starts_with("# My weather config\n") && text.contains("# Home\n"));
        assert!(text.contains("units = \"imperial\""));
        assert!(
            text.contains("[display.gui]\nsections = [\"current\", \"daily\"]"),
            "{text}"
        );
        assert!(
            !text.contains("[display]\n"),
            "the outer table stays implicit"
        );
        let parsed: toml::Table = text.parse().unwrap();
        assert_eq!(parsed["general"]["refresh"].as_str(), Some("30m"));
        replace(&path, &previous).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), COMMENTED);
    }

    #[test]
    fn set_values_refuses_values_in_the_way() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "general = 1\n").unwrap();
        let err = set_values(&path, &[("general.units".into(), "metric".into())]).unwrap_err();
        assert!(matches!(err, EditError::Key(k) if k == "general.units"));
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "general = 1\n",
            "untouched"
        );
        let missing = dir.path().join("new.toml");
        assert_eq!(
            set_values(&missing, &[("general.units".into(), "metric".into())]).unwrap(),
            ""
        );
        assert_eq!(
            fs::read_to_string(&missing).unwrap(),
            "[general]\nunits = \"metric\"\n"
        );
    }

    const COMMENTED: &str = "\
# My weather config
version = 1

[general]
refresh = \"15m\"   # keep this comment

# Home
[[location]]
name = \"New York\"
zip = \"10001\"
country = \"US\"

# [[location]]
# name = \"Berlin\"

[providers]
order = [\"open-meteo\"]
";

    const ADDED: &str = "\
# My weather config
version = 1

[general]
refresh = \"15m\"   # keep this comment

# Home
[[location]]
name = \"New York\"
zip = \"10001\"
country = \"US\"

# added by whether locate
[[location]]
name = \"Cabin\"
query = \"Tromsø, Norway\"
timezone = \"Europe/Oslo\"
provider = \"met-norway\"
elevation = 12.5

# [[location]]
# name = \"Berlin\"

[providers]
order = [\"open-meteo\"]
";

    fn cabin() -> LocationConfig {
        LocationConfig {
            name: Some("Cabin".into()),
            query: Some("Tromsø, Norway".into()),
            timezone: Some("Europe/Oslo".into()),
            provider: Some(ProviderId::MetNorway),
            elevation: Some(12.5),
            ..Default::default()
        }
    }

    fn file_with(text: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, text).unwrap();
        (dir, path)
    }

    #[test]
    fn add_preserves_everything_else() {
        let (_dir, path) = file_with(COMMENTED);
        add_location(&path, &cabin()).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), ADDED);
    }

    #[test]
    fn remove_then_add_round_trips() {
        let (_dir, path) = file_with(ADDED);
        remove_location(&path, "Cabin").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), COMMENTED);
        add_location(&path, &cabin()).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), ADDED);
    }

    #[test]
    fn coordinates_are_written_as_numbers() {
        let (_dir, path) = file_with("");
        let berlin = LocationConfig {
            name: Some("Berlin".into()),
            latitude: Some(52.52),
            longitude: Some(13.41),
            ..Default::default()
        };
        add_location(&path, &berlin).unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "# added by whether locate\n[[location]]\nname = \"Berlin\"\nlatitude = 52.52\nlongitude = 13.41\n"
        );
    }

    #[test]
    fn missing_files_are_created() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("whether/config.toml");
        add_location(&path, &cabin()).unwrap();
        assert!(
            fs::read_to_string(&path)
                .unwrap()
                .contains("name = \"Cabin\"")
        );
    }

    #[test]
    fn duplicates_unknown_names_and_bad_shapes() {
        let (_dir, path) = file_with(COMMENTED);
        let again = LocationConfig {
            name: Some("New York".into()),
            ..cabin()
        };
        assert!(matches!(
            add_location(&path, &again),
            Err(EditError::Duplicate(_))
        ));
        assert!(matches!(
            remove_location(&path, "Nowhere"),
            Err(EditError::NotFound(_))
        ));
        assert!(matches!(
            add_location(&path, &LocationConfig::default()),
            Err(EditError::Unnamed)
        ));
        assert_eq!(fs::read_to_string(&path).unwrap(), COMMENTED);
        let (_dir2, odd) = file_with("location = 3\n");
        assert!(matches!(
            add_location(&odd, &cabin()),
            Err(EditError::Shape(_))
        ));
    }

    #[test]
    fn failed_edits_leave_the_file_untouched() {
        let broken = "version = = 1\n";
        let (dir, path) = file_with(broken);
        assert!(matches!(
            add_location(&path, &cabin()),
            Err(EditError::Parse { .. })
        ));
        assert_eq!(fs::read_to_string(&path).unwrap(), broken);
        let leftovers = fs::read_dir(dir.path()).unwrap().count();
        assert_eq!(leftovers, 1, "no temporary files remain");
    }

    #[test]
    fn atomic_write_failure_keeps_the_original() {
        let (dir, path) = file_with(COMMENTED);
        // A directory where the temporary file would go makes File::create fail.
        let tmp = dir
            .path()
            .join(format!(".config.toml.{}.tmp", std::process::id()));
        fs::create_dir(&tmp).unwrap();
        assert!(matches!(
            add_location(&path, &cabin()),
            Err(EditError::Io { .. })
        ));
        assert_eq!(fs::read_to_string(&path).unwrap(), COMMENTED);
    }

    #[cfg(unix)]
    #[test]
    fn permissions_are_kept() {
        use std::os::unix::fs::PermissionsExt;
        let (_dir, path) = file_with(COMMENTED);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        add_location(&path, &cabin()).unwrap();
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}
