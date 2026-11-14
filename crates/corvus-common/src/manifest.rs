//! `tools.toml` — the launcher's catalogue.
//!
//! Design rule from the brief: a missing or malformed entry must never take the launcher
//! down. So parsing is done *per entry*: the file is read as a generic TOML table, and each
//! `[[tool]]` is deserialized on its own. Bad entries become warnings, good ones load.

use serde::Deserialize;
use std::path::{Component, Path, PathBuf};

/// Where a tool is allowed to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Only on a live (installed) Windows.
    Live,
    /// Only inside WinPE (Corvus PE).
    Pe,
    /// Anywhere.
    #[default]
    Both,
}

impl Mode {
    /// Is a tool with this mode usable in the current environment?
    pub fn allowed_in(self, in_pe: bool) -> bool {
        match self {
            Mode::Both => true,
            Mode::Pe => in_pe,
            Mode::Live => !in_pe,
        }
    }
}

/// One `[[tool]]` entry. Unknown keys are ignored so the schema can grow without
/// breaking older launchers.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Tool {
    pub id: String,
    pub name: String,
    /// Tab / section in the launcher.
    pub category: String,
    /// Relative to the drive root, forward or back slashes. Never absolute, never `..`.
    pub path: String,
    #[serde(default)]
    pub args: String,
    /// Optional icon path (relative to drive root). Empty = extract from the exe.
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub mode: Mode,
    #[serde(default)]
    pub version: String,
    /// Official download page the copy came from.
    #[serde(default)]
    pub source: String,
    /// Lower-case hex SHA-256 of the exe. Empty = don't check.
    #[serde(default)]
    pub sha256: String,
    /// Licence name shown on the credits page (e.g. "GPL-3.0", "Freeware").
    #[serde(default)]
    pub license: String,
    /// Launch via the UAC "runas" verb. The launcher itself never elevates.
    #[serde(default)]
    pub elevate: bool,
    /// Free text shown as a tooltip / in credits.
    #[serde(default)]
    pub notes: String,
}

impl Tool {
    /// Full path of the exe on a drive rooted at `root`.
    pub fn resolve(&self, root: &Path) -> PathBuf {
        root.join(self.path.replace('/', "\\"))
    }

    /// Validate the fields that can't be checked by serde alone.
    fn validate(&self) -> Result<(), String> {
        if self.id.trim().is_empty() {
            return Err("id is empty".into());
        }
        if self.name.trim().is_empty() {
            return Err(format!("tool '{}': name is empty", self.id));
        }
        if self.category.trim().is_empty() {
            return Err(format!("tool '{}': category is empty", self.id));
        }
        validate_relative_path(&self.path).map_err(|e| format!("tool '{}': {e}", self.id))?;
        if !self.icon.is_empty() {
            validate_relative_path(&self.icon)
                .map_err(|e| format!("tool '{}': icon {e}", self.id))?;
        }
        if !self.sha256.is_empty()
            && (self.sha256.len() != 64 || !self.sha256.chars().all(|c| c.is_ascii_hexdigit()))
        {
            return Err(format!(
                "tool '{}': sha256 is not 64 hex characters",
                self.id
            ));
        }
        Ok(())
    }
}

/// A path is acceptable if it is non-empty, relative to the drive root, and never climbs out.
fn validate_relative_path(p: &str) -> Result<(), String> {
    if p.trim().is_empty() {
        return Err("path is empty".into());
    }
    let normalized = p.replace('/', "\\");
    let path = Path::new(&normalized);
    if path.is_absolute() || normalized.starts_with('\\') || normalized.contains(':') {
        return Err(format!("path '{p}' must be relative to the drive root"));
    }
    for comp in path.components() {
        match comp {
            Component::ParentDir => return Err(format!("path '{p}' must not contain '..'")),
            Component::Prefix(_) | Component::RootDir => {
                return Err(format!("path '{p}' must be relative to the drive root"))
            }
            _ => {}
        }
    }
    Ok(())
}

/// Result of checking a tool against the drive and the current environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolStatus {
    /// Exe is present (and hash matched, if one was given).
    Ready,
    /// Exe is not on the drive. Tile greys out, tagged "missing".
    Missing,
    /// Exe is present but its SHA-256 differs from `tools.toml`. Tagged "changed".
    Changed,
    /// Tool exists but is for the other environment (PE-only on live, or vice versa).
    WrongMode,
}

impl ToolStatus {
    pub fn is_launchable(self) -> bool {
        matches!(self, ToolStatus::Ready | ToolStatus::Changed)
    }

    /// Short tag for the tile.
    pub fn tag(self) -> &'static str {
        match self {
            ToolStatus::Ready => "",
            ToolStatus::Missing => "missing",
            ToolStatus::Changed => "changed",
            ToolStatus::WrongMode => "n/a here",
        }
    }
}

/// Check one tool. `verify_hash` controls whether a non-empty `sha256` is actually computed
/// (hashing large exes takes time; the launcher does it off the UI thread).
pub fn check_tool(tool: &Tool, root: &Path, in_pe: bool, verify_hash: bool) -> ToolStatus {
    if !tool.mode.allowed_in(in_pe) {
        return ToolStatus::WrongMode;
    }
    let exe = tool.resolve(root);
    if !exe.is_file() {
        return ToolStatus::Missing;
    }
    if verify_hash && !tool.sha256.is_empty() {
        match crate::hash::sha256_file(&exe) {
            Ok(actual) if actual.eq_ignore_ascii_case(&tool.sha256) => ToolStatus::Ready,
            Ok(_) => ToolStatus::Changed,
            Err(e) => {
                log::warn!("could not hash {}: {e}", exe.display());
                ToolStatus::Ready
            }
        }
    } else {
        ToolStatus::Ready
    }
}

/// The parsed catalogue. `warnings` holds every entry that was skipped and why.
#[derive(Debug, Default, Clone)]
pub struct Manifest {
    pub tools: Vec<Tool>,
    pub warnings: Vec<String>,
}

impl Manifest {
    /// Parse TOML text. Never fails: unparseable input yields an empty manifest with a warning.
    pub fn parse(text: &str) -> Manifest {
        let mut m = Manifest::default();
        let table: toml::Table = match toml::from_str(text) {
            Ok(t) => t,
            Err(e) => {
                m.warnings
                    .push(format!("tools.toml is not valid TOML: {e}"));
                return m;
            }
        };
        let Some(entries) = table.get("tool") else {
            m.warnings.push("tools.toml has no [[tool]] entries".into());
            return m;
        };
        let Some(entries) = entries.as_array() else {
            m.warnings
                .push("'tool' must be an array of tables ([[tool]]), not a single value".into());
            return m;
        };
        for (i, entry) in entries.iter().enumerate() {
            let n = i + 1;
            let tool: Tool = match entry.clone().try_into() {
                Ok(t) => t,
                Err(e) => {
                    m.warnings.push(format!("[[tool]] #{n} skipped: {e}"));
                    continue;
                }
            };
            if let Err(e) = tool.validate() {
                m.warnings.push(format!("[[tool]] #{n} skipped: {e}"));
                continue;
            }
            if m.tools.iter().any(|t| t.id == tool.id) {
                m.warnings
                    .push(format!("[[tool]] #{n} skipped: duplicate id '{}'", tool.id));
                continue;
            }
            m.tools.push(tool);
        }
        for w in &m.warnings {
            log::warn!("{w}");
        }
        m
    }

    /// Read and parse a file. A missing or unreadable file yields an empty manifest plus a warning.
    pub fn load(path: &Path) -> Manifest {
        match std::fs::read_to_string(path) {
            Ok(text) => Manifest::parse(&text),
            Err(e) => {
                let w = format!("cannot read {}: {e}", path.display());
                log::warn!("{w}");
                Manifest {
                    tools: Vec::new(),
                    warnings: vec![w],
                }
            }
        }
    }

    /// Categories in first-seen order.
    pub fn categories(&self) -> Vec<&str> {
        let mut out: Vec<&str> = Vec::new();
        for t in &self.tools {
            if !out.contains(&t.category.as_str()) {
                out.push(&t.category);
            }
        }
        out
    }

    pub fn tools_in<'a>(&'a self, category: &'a str) -> impl Iterator<Item = &'a Tool> + 'a {
        self.tools.iter().filter(move |t| t.category == category)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"
[[tool]]
id = "hwinfo"
name = "HWiNFO"
category = "Diagnostics"
path = "Tools/Diagnostics/HWiNFO/HWiNFO64.exe"
version = "8.x"
source = "https://www.hwinfo.com/download/"

[[tool]]
id = "procmon"
name = "Process Monitor"
category = "System"
path = "Tools\\System\\Sysinternals\\Procmon64.exe"
mode = "live"
elevate = true
"#;

    #[test]
    fn parses_good_file_with_defaults() {
        let m = Manifest::parse(GOOD);
        assert!(m.warnings.is_empty(), "{:?}", m.warnings);
        assert_eq!(m.tools.len(), 2);
        let t = &m.tools[0];
        assert_eq!(t.id, "hwinfo");
        assert_eq!(t.mode, Mode::Both);
        assert_eq!(t.args, "");
        assert_eq!(t.sha256, "");
        assert!(!t.elevate);
        assert_eq!(m.tools[1].mode, Mode::Live);
        assert!(m.tools[1].elevate);
        assert_eq!(m.categories(), vec!["Diagnostics", "System"]);
    }

    #[test]
    fn resolve_normalizes_slashes() {
        let m = Manifest::parse(GOOD);
        let p = m.tools[0].resolve(Path::new("E:\\"));
        assert_eq!(
            p,
            PathBuf::from("E:\\Tools\\Diagnostics\\HWiNFO\\HWiNFO64.exe")
        );
    }

    #[test]
    fn missing_required_field_skips_only_that_entry() {
        let text =
            format!("{GOOD}\n[[tool]]\nid = \"nopath\"\nname = \"No Path\"\ncategory = \"X\"\n");
        let m = Manifest::parse(&text);
        assert_eq!(m.tools.len(), 2);
        assert_eq!(m.warnings.len(), 1);
        assert!(m.warnings[0].contains("#3"), "{}", m.warnings[0]);
        assert!(m.warnings[0].contains("path"), "{}", m.warnings[0]);
    }

    #[test]
    fn bad_paths_are_rejected() {
        for bad in [
            "C:\\Windows\\System32\\cmd.exe",
            "\\Tools\\x.exe",
            "/Tools/x.exe",
            "Tools/../../x.exe",
            "..\\x.exe",
            "",
            "   ",
        ] {
            let text =
                format!("[[tool]]\nid = \"t\"\nname = \"T\"\ncategory = \"C\"\npath = '{bad}'\n");
            let m = Manifest::parse(&text);
            assert!(m.tools.is_empty(), "path {bad:?} should be rejected");
            assert_eq!(m.warnings.len(), 1, "path {bad:?}");
        }
    }

    #[test]
    fn unknown_mode_skips_entry() {
        let text = "[[tool]]\nid = \"t\"\nname = \"T\"\ncategory = \"C\"\npath = \"x.exe\"\nmode = \"sometimes\"\n";
        let m = Manifest::parse(text);
        assert!(m.tools.is_empty());
        assert!(m.warnings[0].contains("sometimes"));
    }

    #[test]
    fn unknown_keys_are_ignored() {
        let text = "[[tool]]\nid = \"t\"\nname = \"T\"\ncategory = \"C\"\npath = \"x.exe\"\nfuture_key = 42\n";
        let m = Manifest::parse(text);
        assert_eq!(m.tools.len(), 1);
        assert!(m.warnings.is_empty());
    }

    #[test]
    fn duplicate_id_keeps_first() {
        let text = "[[tool]]\nid = \"t\"\nname = \"First\"\ncategory = \"C\"\npath = \"a.exe\"\n\
                    [[tool]]\nid = \"t\"\nname = \"Second\"\ncategory = \"C\"\npath = \"b.exe\"\n";
        let m = Manifest::parse(text);
        assert_eq!(m.tools.len(), 1);
        assert_eq!(m.tools[0].name, "First");
        assert!(m.warnings[0].contains("duplicate"));
    }

    #[test]
    fn bad_sha_is_rejected_good_sha_accepted() {
        let base = "[[tool]]\nid = \"t\"\nname = \"T\"\ncategory = \"C\"\npath = \"x.exe\"\n";
        let m = Manifest::parse(&format!("{base}sha256 = \"abc\"\n"));
        assert!(m.tools.is_empty());
        let m = Manifest::parse(&format!("{base}sha256 = \"{}\"\n", "a".repeat(64)));
        assert_eq!(m.tools.len(), 1);
    }

    #[test]
    fn garbage_and_empty_input_never_panic() {
        for text in [
            "",
            "this is not toml = = =",
            "[[tool]]\n",
            "tool = 5",
            "[tool]\nid='x'",
        ] {
            let m = Manifest::parse(text);
            assert!(m.tools.is_empty(), "{text:?}");
            assert!(!m.warnings.is_empty(), "{text:?} should warn");
        }
    }

    #[test]
    fn load_missing_file_warns() {
        let m = Manifest::load(Path::new("Z:\\definitely\\not\\here\\tools.toml"));
        assert!(m.tools.is_empty());
        assert_eq!(m.warnings.len(), 1);
    }

    #[test]
    fn mode_gating() {
        assert!(Mode::Both.allowed_in(true) && Mode::Both.allowed_in(false));
        assert!(Mode::Pe.allowed_in(true) && !Mode::Pe.allowed_in(false));
        assert!(Mode::Live.allowed_in(false) && !Mode::Live.allowed_in(true));
    }

    #[test]
    fn check_tool_statuses() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("Tools")).unwrap();
        std::fs::write(root.join("Tools").join("real.exe"), b"hello corvus").unwrap();
        let real_hash = crate::hash::sha256_file(&root.join("Tools/real.exe")).unwrap();

        let mut t = Tool {
            id: "t".into(),
            name: "T".into(),
            category: "C".into(),
            path: "Tools/real.exe".into(),
            args: String::new(),
            icon: String::new(),
            mode: Mode::Both,
            version: String::new(),
            source: String::new(),
            sha256: String::new(),
            license: String::new(),
            elevate: false,
            notes: String::new(),
        };
        assert_eq!(check_tool(&t, root, false, true), ToolStatus::Ready);

        t.sha256 = real_hash.clone();
        assert_eq!(check_tool(&t, root, false, true), ToolStatus::Ready);

        t.sha256 = "0".repeat(64);
        assert_eq!(check_tool(&t, root, false, true), ToolStatus::Changed);
        assert_eq!(
            check_tool(&t, root, false, false),
            ToolStatus::Ready,
            "hash not verified when verify_hash=false"
        );

        t.path = "Tools/gone.exe".into();
        assert_eq!(check_tool(&t, root, false, true), ToolStatus::Missing);

        t.path = "Tools/real.exe".into();
        t.sha256.clear();
        t.mode = Mode::Pe;
        assert_eq!(check_tool(&t, root, false, true), ToolStatus::WrongMode);
        assert_eq!(check_tool(&t, root, true, true), ToolStatus::Ready);

        // A directory with the exe's name is not an exe.
        std::fs::create_dir_all(root.join("Tools").join("dir.exe")).unwrap();
        t.mode = Mode::Both;
        t.path = "Tools/dir.exe".into();
        assert_eq!(check_tool(&t, root, false, true), ToolStatus::Missing);
    }
}
