//! `cargo xtask ios licenses`: the open-source notices the iPhone app shows.
//!
//! Walks the normal (not build or dev) dependencies of `whether-ffi` for
//! `aarch64-apple-ios`, reads each crate's license files from the cargo
//! registry, and writes `ios/Whether/Whether/Acknowledgements.json`. Identical
//! texts are stored once.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::workspace_root;

/// The package the app links.
const ROOT: &str = "whether-ffi";
/// The platform whose dependency tree ships.
const PLATFORM: &str = "aarch64-apple-ios";
/// The committed notices, relative to the repository.
pub(crate) const OUTPUT: &str = "ios/Whether/Whether/Acknowledgements.json";

/// One crate in the shipped tree.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Package {
    pub name: String,
    pub version: String,
    pub license: String,
    pub authors: String,
    dir: PathBuf,
}

pub(crate) fn run() -> Result<(), String> {
    let root = workspace_root();
    let meta = metadata()?;
    let packages = shipped(&meta)?;
    let text = render(&packages)?;
    std::fs::write(root.join(OUTPUT), text).map_err(|e| format!("writing {OUTPUT}: {e}"))?;
    println!("wrote {} crates to {OUTPUT}", packages.len());
    Ok(())
}

/// `cargo metadata` for the iPhone, from the lock file alone.
pub(crate) fn metadata() -> Result<Value, String> {
    let out = std::process::Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--locked", "--offline"])
        .args(["--filter-platform", PLATFORM])
        .current_dir(workspace_root())
        .output()
        .map_err(|e| format!("failed to start `cargo metadata`: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "cargo metadata: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    serde_json::from_slice(&out.stdout).map_err(|e| format!("cargo metadata: {e}"))
}

/// The third-party crates reachable from `whether-ffi` through normal
/// dependencies, sorted by name and version. Path crates (the project's own)
/// are left out: their license is shown first.
pub(crate) fn shipped(meta: &Value) -> Result<Vec<Package>, String> {
    let packages: BTreeMap<&str, &Value> = meta["packages"]
        .as_array()
        .ok_or("no packages")?
        .iter()
        .filter_map(|p| Some((p["id"].as_str()?, p)))
        .collect();
    let nodes: BTreeMap<&str, &Value> = meta["resolve"]["nodes"]
        .as_array()
        .ok_or("no resolve")?
        .iter()
        .filter_map(|n| Some((n["id"].as_str()?, n)))
        .collect();
    let root = packages
        .iter()
        .find(|(_, p)| p["name"] == ROOT)
        .map(|(id, _)| *id)
        .ok_or_else(|| format!("no package {ROOT}"))?;
    let mut seen = BTreeSet::new();
    let mut todo = vec![root];
    while let Some(id) = todo.pop() {
        if !seen.insert(id) {
            continue;
        }
        for dep in nodes[id]["deps"].as_array().into_iter().flatten() {
            let normal = dep["dep_kinds"]
                .as_array()
                .is_some_and(|kinds| kinds.iter().any(|k| k["kind"].is_null()));
            if let (true, Some(pkg)) = (normal, dep["pkg"].as_str()) {
                todo.push(pkg);
            }
        }
    }
    let mut out: Vec<Package> = seen
        .into_iter()
        .map(|id| packages[id])
        .filter(|p| !p["source"].is_null())
        .map(|p| Package {
            name: p["name"].as_str().unwrap_or_default().to_owned(),
            version: p["version"].as_str().unwrap_or_default().to_owned(),
            license: p["license"].as_str().unwrap_or_default().to_owned(),
            authors: p["authors"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default(),
            dir: Path::new(p["manifest_path"].as_str().unwrap_or_default())
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_default(),
        })
        .collect();
    out.sort();
    Ok(out)
}

/// The crate's own license files (`LICENSE*`, `LICENCE*`, `COPYING*`,
/// `UNLICENSE*` at its top), by file name, joined.
fn license_files(dir: &Path) -> Option<String> {
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_uppercase();
            ["LICENSE", "LICENCE", "COPYING", "UNLICENSE"]
                .iter()
                .any(|p| name.starts_with(p))
                && e.path().is_file()
        })
        .map(|e| e.path())
        .collect();
    files.sort();
    let texts: Vec<String> = files
        .iter()
        .filter_map(|f| std::fs::read_to_string(f).ok())
        .map(|t| t.trim().to_owned())
        .filter(|t| !t.is_empty())
        .collect();
    (!texts.is_empty()).then(|| texts.join("\n\n---\n\n"))
}

/// A standard text for the first license of `expression` the table knows, for
/// crates that ship none.
fn fallback(expression: &str, authors: &str) -> Option<String> {
    let holders = if authors.is_empty() {
        "the authors"
    } else {
        authors
    };
    expression
        .split(|c: char| !(c.is_alphanumeric() || matches!(c, '-' | '.')))
        .find_map(|id| match id {
            "MPL-2.0" => Some(include_str!("../data/licenses/MPL-2.0.txt").to_owned()),
            "Apache-2.0" => Some(include_str!("../data/licenses/Apache-2.0.txt").to_owned()),
            "Unicode-3.0" => Some(include_str!("../data/licenses/Unicode-3.0.txt").to_owned()),
            "MIT" => Some(format!(
                "MIT License\n\nCopyright (c) {holders}\n\nPermission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the \"Software\"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:\n\nThe above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.\n\nTHE SOFTWARE IS PROVIDED \"AS IS\", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE."
            )),
            "ISC" => Some(format!(
                "ISC License\n\nCopyright (c) {holders}\n\nPermission to use, copy, modify, and/or distribute this software for any purpose with or without fee is hereby granted, provided that the above copyright notice and this permission notice appear in all copies.\n\nTHE SOFTWARE IS PROVIDED \"AS IS\" AND THE AUTHOR DISCLAIMS ALL WARRANTIES WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE."
            )),
            "Zlib" => Some(format!(
                "zlib License\n\nCopyright (c) {holders}\n\nThis software is provided 'as-is', without any express or implied warranty. In no event will the authors be held liable for any damages arising from the use of this software.\n\nPermission is granted to anyone to use this software for any purpose, including commercial applications, and to alter it and redistribute it freely, subject to the following restrictions:\n\n1. The origin of this software must not be misrepresented; you must not claim that you wrote the original software. If you use this software in a product, an acknowledgment in the product documentation would be appreciated but is not required.\n2. Altered source versions must be plainly marked as such, and must not be misrepresented as being the original software.\n3. This notice may not be removed or altered from any source distribution."
            )),
            "BSD-3-Clause" => Some(format!(
                "BSD 3-Clause License\n\nCopyright (c) {holders}\n\nRedistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met:\n\n1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following disclaimer.\n\n2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following disclaimer in the documentation and/or other materials provided with the distribution.\n\n3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote products derived from this software without specific prior written permission.\n\nTHIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS \"AS IS\" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE."
            )),
            _ => None,
        })
}

/// The JSON: `crates` (name, version, license, text id) and `texts`, each
/// distinct text once under its id.
fn render(packages: &[Package]) -> Result<String, String> {
    let mut ids: BTreeMap<String, usize> = BTreeMap::new();
    let mut crates = Vec::new();
    for p in packages {
        let text = license_files(&p.dir)
            .or_else(|| fallback(&p.license, &p.authors))
            .ok_or_else(|| {
                format!(
                    "{} {}: no license text for `{}`",
                    p.name, p.version, p.license
                )
            })?;
        let next = ids.len();
        let id = *ids.entry(text).or_insert(next);
        crates.push(json!({
            "name": p.name, "version": p.version, "license": p.license, "text": id,
        }));
    }
    let mut texts = vec![String::new(); ids.len()];
    for (text, id) in ids {
        texts[id] = text;
    }
    let mut out = serde_json::to_string_pretty(&json!({ "crates": crates, "texts": texts }))
        .map_err(|e| e.to_string())?;
    out.push('\n');
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_license_files_in_order() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("LICENSE-MIT"), "mit text\n").unwrap();
        std::fs::write(dir.path().join("LICENSE-APACHE"), "apache text").unwrap();
        std::fs::write(dir.path().join("README.md"), "not a license").unwrap();
        assert_eq!(
            license_files(dir.path()).unwrap(),
            "apache text\n\n---\n\nmit text"
        );
        let empty = tempfile::tempdir().unwrap();
        assert_eq!(license_files(empty.path()), None);
    }

    #[test]
    fn fallback_picks_the_first_known_license() {
        let mit = fallback("Unlicense OR MIT", "A. Author").unwrap();
        assert!(mit.contains("Copyright (c) A. Author"));
        assert!(
            fallback("MPL-2.0", "")
                .unwrap()
                .contains("Mozilla Public License")
        );
        assert!(
            fallback("(MIT OR Apache-2.0) AND Unicode-3.0", "")
                .unwrap()
                .starts_with("MIT")
        );
        assert_eq!(fallback("Proprietary", ""), None);
    }

    #[test]
    fn identical_texts_are_stored_once_and_a_missing_text_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("LICENSE"), "same").unwrap();
        let pkg = |name: &str, license: &str| Package {
            name: name.into(),
            version: "1.0.0".into(),
            license: license.into(),
            authors: String::new(),
            dir: dir.path().to_path_buf(),
        };
        let json: Value =
            serde_json::from_str(&render(&[pkg("a", "MIT"), pkg("b", "MIT")]).unwrap()).unwrap();
        assert_eq!(json["texts"].as_array().unwrap().len(), 1);
        assert_eq!(json["crates"][1]["text"], 0);
        let none = Package {
            dir: PathBuf::from("/nonexistent"),
            ..pkg("c", "Proprietary")
        };
        assert!(render(&[none]).is_err());
    }

    /// Fails when `Cargo.lock` changed and `cargo xtask ios licenses` was not run.
    #[test]
    fn committed_acknowledgements_match_the_lock_file() {
        // Needs the iPhone target's crates in the local registry (fetched
        // already on a development machine); without them there is nothing to compare.
        let Ok(meta) = metadata() else {
            eprintln!("skipped: cargo metadata for {PLATFORM} is unavailable offline");
            return;
        };
        let wanted: Vec<String> = shipped(&meta)
            .unwrap()
            .iter()
            .map(|p| format!("{} {}", p.name, p.version))
            .collect();
        let file = std::fs::read_to_string(workspace_root().join(OUTPUT)).unwrap();
        let json: Value = serde_json::from_str(&file).unwrap();
        let have: Vec<String> = json["crates"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| {
                format!(
                    "{} {}",
                    c["name"].as_str().unwrap(),
                    c["version"].as_str().unwrap()
                )
            })
            .collect();
        assert_eq!(have, wanted, "run `cargo xtask ios licenses`");
    }
}
