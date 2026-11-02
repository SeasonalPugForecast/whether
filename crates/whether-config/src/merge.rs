//! Layering raw TOML: tables deep-merge, everything else (arrays included) replaces.

use alloc::format;
use alloc::string::{String, ToString};

use toml::{Table, Value};

/// Merges `overlay` into `base`. Tables merge key by key; any other value in
/// `overlay`, including arrays such as `[[location]]`, replaces the one in `base`.
pub fn deep_merge(base: &mut Table, overlay: Table) {
    for (key, value) in overlay {
        match (base.get_mut(&key), value) {
            (Some(Value::Table(existing)), Value::Table(incoming)) => {
                deep_merge(existing, incoming);
            }
            (_, value) => {
                base.insert(key, value);
            }
        }
    }
}

/// Sets a dotted key such as `general.units` or `display.tui.layout`, creating
/// intermediate tables. Fails if an intermediate key holds a non-table value.
pub fn set_dotted(table: &mut Table, key: &str, value: Value) -> Result<(), String> {
    if key.is_empty() || key.split('.').any(str::is_empty) {
        return Err(format!("`{key}` is not a dotted key"));
    }
    let mut parts = key.split('.').peekable();
    let mut current = table;
    while let Some(part) = parts.next() {
        if parts.peek().is_none() {
            current.insert(part.to_string(), value);
            return Ok(());
        }
        let entry = current
            .entry(part.to_string())
            .or_insert_with(|| Value::Table(Table::new()));
        current = match entry {
            Value::Table(t) => t,
            _ => return Err(format!("`{part}` in `{key}` is not a table")),
        };
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(text: &str) -> Table {
        text.parse().unwrap()
    }

    #[test]
    fn tables_merge_and_scalars_replace() {
        let mut base =
            table("[general]\nrefresh = \"15m\"\nunits = \"auto\"\n[display]\ndefault = \"tui\"");
        deep_merge(
            &mut base,
            table("[general]\nunits = \"metric\"\n[runtime]\nlog_level = \"debug\""),
        );
        assert_eq!(
            base,
            table(
                "[general]\nrefresh = \"15m\"\nunits = \"metric\"\n[display]\ndefault = \"tui\"\n\
                 [runtime]\nlog_level = \"debug\""
            )
        );
    }

    #[test]
    fn arrays_replace() {
        let mut base = table(
            "[[location]]\nname = \"a\"\n[[location]]\nname = \"b\"\n[providers]\norder = [\"openweather\", \"open-meteo\"]",
        );
        deep_merge(
            &mut base,
            table("[[location]]\nname = \"c\"\n[providers]\norder = [\"open-meteo\"]"),
        );
        assert_eq!(
            base,
            table("[[location]]\nname = \"c\"\n[providers]\norder = [\"open-meteo\"]")
        );
    }

    #[test]
    fn a_table_can_replace_a_scalar_and_back() {
        let mut base = table("a = 1\nb = { c = 2 }");
        deep_merge(&mut base, table("a = { x = 1 }\nb = 3"));
        assert_eq!(base, table("a = { x = 1 }\nb = 3"));
    }

    #[test]
    fn dotted_keys_create_tables() {
        let mut t = table("[general]\nrefresh = \"15m\"");
        set_dotted(&mut t, "general.units", Value::String("imperial".into())).unwrap();
        set_dotted(&mut t, "display.tui.layout", Value::String("wide".into())).unwrap();
        set_dotted(&mut t, "version", Value::Integer(1)).unwrap();
        assert_eq!(
            t,
            table(
                "version = 1\n[general]\nrefresh = \"15m\"\nunits = \"imperial\"\n\
                 [display.tui]\nlayout = \"wide\""
            )
        );
    }

    #[test]
    fn dotted_keys_reject_bad_paths() {
        let mut t = table("version = 1");
        assert!(
            set_dotted(&mut t, "version.x", Value::Integer(2))
                .unwrap_err()
                .contains("not a table")
        );
        assert!(set_dotted(&mut t, "", Value::Integer(2)).is_err());
        assert!(set_dotted(&mut t, "a..b", Value::Integer(2)).is_err());
    }
}
