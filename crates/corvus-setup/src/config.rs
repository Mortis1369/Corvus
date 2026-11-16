//! The small files CorvusSetup writes onto the drive after the payload copy:
//!
//! * `ventoy\ventoy.json`   — Ventoy's configuration (menu names, theme, a few switches)
//! * `ventoy\ventoy-log.md` — every one of those settings in plain English, with why and when
//! * `autorun.inf`          — drive icon and name in Explorer
//! * `.ventoyignore`        — marker files that keep Ventoy from scanning the tool folders
//!
//! Ventoy stays a black box for Aiden: the settings live in the tables below, and both the
//! JSON and the log are generated from the same tables so they cannot drift apart.
//! Keys were checked against ventoy.net's plugin pages on 2026-11-14 (see docs/ventoy-notes.md).

use serde::Deserialize;
use std::fs;
use std::path::Path;

/// One line of Ventoy configuration, with its explanation.
pub struct Setting {
    pub key: &'static str,
    pub value: &'static str,
    pub what: &'static str,
    pub why: &'static str,
    /// Date this value was last changed in CorvusSetup.
    pub since: &'static str,
}

/// `control` plugin: global switches.
pub const CONTROL: &[Setting] = &[
    Setting {
        key: "VTOY_DEFAULT_MENU_MODE",
        value: "0",
        what: "Boot menu opens as one flat list (F3 switches to folder view).",
        why: "A handful of boot tools reads fastest as a single list.",
        since: "2026-11-14",
    },
    Setting {
        key: "VTOY_FILT_DOT_UNDERSCORE_FILE",
        value: "1",
        what: "Hides files whose names start with ._",
        why: "A Mac leaves ._ junk files beside ISOs; they are not bootable and only clutter the menu.",
        since: "2026-11-14",
    },
    Setting {
        key: "VTOY_WIN11_BYPASS_CHECK",
        value: "1",
        what: "Windows 11 installer ISOs skip the TPM / Secure Boot / CPU / RAM checks.",
        why: "Lets Windows 11 be installed on older bench machines. This is also Ventoy's default; written out so it is visible.",
        since: "2026-11-14",
    },
    Setting {
        key: "VTOY_WIN11_BYPASS_NRO",
        value: "1",
        what: "Windows 11 setup allows a local account without a network connection.",
        why: "Bench installs often have no network yet. Also Ventoy's default; written out so it is visible.",
        since: "2026-11-14",
    },
    Setting {
        key: "VTOY_SECURE_BOOT_POLICY",
        value: "0",
        what: "Ventoy boots images without applying the firmware's Secure Boot check to them.",
        why: "Ventoy's default. Several rescue images (ShredOS, Memtest86+) are not signed and would refuse to start otherwise.",
        since: "2026-11-14",
    },
];

/// `theme` plugin.
pub const THEME: &[Setting] = &[
    Setting {
        key: "file",
        value: "/ventoy/theme/corvus/theme.txt",
        what: "The menu's look (a GRUB2 theme file).",
        why: "Dark Corvus theme. To change the picture, replace ventoy\\theme\\corvus\\background.png — nothing else.",
        since: "2026-11-14",
    },
    Setting {
        key: "gfxmode",
        value: "max",
        what: "Menu uses the highest resolution the firmware offers.",
        why: "Ventoy's default is 1024x768, which looks stretched on modern screens.",
        since: "2026-11-14",
    },
    Setting {
        key: "display_mode",
        value: "GUI",
        what: "Graphical menu (F7 switches to plain text if a machine's graphics misbehave).",
        why: "Ventoy's default; written out so it is visible.",
        since: "2026-11-14",
    },
    Setting {
        key: "ventoy_left",
        value: "5%",
        what: "Horizontal position of the small Ventoy version text.",
        why: "Keeps it in the bottom-left corner, out of the way.",
        since: "2026-11-14",
    },
    Setting {
        key: "ventoy_top",
        value: "95%",
        what: "Vertical position of the small Ventoy version text.",
        why: "Keeps it in the bottom-left corner, out of the way.",
        since: "2026-11-14",
    },
    Setting {
        key: "ventoy_color",
        value: "#808080",
        what: "Colour of the Ventoy version text.",
        why: "Grey instead of Ventoy's default blue, to match the dark theme.",
        since: "2026-11-14",
    },
];

/// Folders Ventoy must not scan for bootable images.
pub const IGNORED_DIRS: &[&str] = &["Tools", "Launcher", "Branding"];
/// Folder names shown in Ventoy's folder view (F3).
pub const DIR_ALIASES: &[(&str, &str)] = &[
    ("/Boot", "[Boot tools]"),
    ("/Installers", "[Installers]"),
    ("/Installers/Windows", "[Windows installers]"),
    ("/Installers/Linux", "[Linux installers]"),
];
pub const PE_IMAGE: &str = "Boot/CorvusPE.iso";
pub const PE_ALIAS: &str = "Corvus PE";

/// `Launcher\boot-menu.toml`, written by stage-payload.
#[derive(Deserialize, Default)]
struct BootMenuFile {
    #[serde(default)]
    image: Vec<BootImage>,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct BootImage {
    /// Relative to the drive root, forward slashes: `Boot/Lockpick.wim`.
    pub path: String,
    pub alias: String,
}

/// Friendly names for the boot images. Missing or broken file → just the Corvus PE entry.
pub fn load_boot_menu(root: &Path) -> (Vec<BootImage>, Option<String>) {
    let path = root.join("Launcher").join("boot-menu.toml");
    let mut warn = None;
    let mut images = match fs::read_to_string(&path) {
        Ok(text) => match toml::from_str::<BootMenuFile>(&text) {
            Ok(f) => f.image,
            Err(e) => {
                warn = Some(format!(
                    "boot-menu.toml is not valid ({e}); menu shows file names"
                ));
                Vec::new()
            }
        },
        Err(_) => {
            warn = Some("boot-menu.toml not found; menu shows file names".to_string());
            Vec::new()
        }
    };
    if !images.iter().any(|i| i.path.eq_ignore_ascii_case(PE_IMAGE)) {
        images.insert(
            0,
            BootImage {
                path: PE_IMAGE.to_string(),
                alias: PE_ALIAS.to_string(),
            },
        );
    }
    (images, warn)
}

/// JSON string literal with the escapes JSON requires.
fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Build `ventoy.json`. `default_image` (e.g. `/Boot/CorvusPE.iso`) is pre-selected in the menu
/// when given.
pub fn ventoy_json(images: &[BootImage], default_image: Option<&str>) -> String {
    let mut control: Vec<String> = CONTROL
        .iter()
        .map(|s| format!("    {{ {}: {} }}", json_str(s.key), json_str(s.value)))
        .collect();
    if let Some(img) = default_image {
        control.push(format!(
            "    {{ {}: {} }}",
            json_str("VTOY_DEFAULT_IMAGE"),
            json_str(img)
        ));
    }
    let theme: Vec<String> = THEME
        .iter()
        .map(|s| format!("    {}: {}", json_str(s.key), json_str(s.value)))
        .collect();
    let mut alias: Vec<String> = images
        .iter()
        .map(|i| {
            format!(
                "    {{ \"image\": {}, \"alias\": {} }}",
                json_str(&format!("/{}", i.path.trim_start_matches('/'))),
                json_str(&i.alias)
            )
        })
        .collect();
    alias.extend(DIR_ALIASES.iter().map(|(dir, name)| {
        format!(
            "    {{ \"dir\": {}, \"alias\": {} }}",
            json_str(dir),
            json_str(name)
        )
    }));
    format!(
        "{{\n  \"control\": [\n{}\n  ],\n  \"theme\": {{\n{}\n  }},\n  \"menu_alias\": [\n{}\n  ]\n}}\n",
        control.join(",\n"),
        theme.join(",\n"),
        alias.join(",\n")
    )
}

pub fn autorun_inf() -> String {
    "[autorun]\r\nicon=Branding\\karasu.ico\r\nlabel=CORVUS\r\n".to_string()
}

/// What happened in this run, for the log's history section.
pub struct RunNote<'a> {
    pub when: &'a str,
    pub mode: &'a str,
    pub ventoy_version: &'a str,
    /// Exact Ventoy command line, or empty when Ventoy was not run.
    pub ventoy_command: &'a str,
}

const HISTORY_HEADING: &str = "## History";

/// Build `ventoy-log.md`. `previous` is the existing file's text, whose history lines are kept.
pub fn ventoy_log_md(
    images: &[BootImage],
    default_image: Option<&str>,
    note: &RunNote,
    previous: Option<&str>,
) -> String {
    let mut md = String::new();
    md.push_str("# Ventoy on this drive — what is set, and why\n\n");
    md.push_str(&format!(
        "Written by Corvus Setup {} on {} (UTC). Ventoy version: {}.\n\n",
        env!("CARGO_PKG_VERSION"),
        note.when,
        if note.ventoy_version.is_empty() {
            "unknown"
        } else {
            note.ventoy_version
        }
    ));
    md.push_str(
        "Ventoy is the boot loader that shows the menu when a PC starts from this drive. You never \
         need to touch it. This file exists so that if you ever get curious, every choice is \
         written down. The settings live in `ventoy\\ventoy.json`; Corvus Setup rewrites both \
         files on every run, so edit Corvus Setup's tables (`crates/corvus-setup/src/config.rs`) \
         rather than the files on the drive.\n\n",
    );

    md.push_str("## How the drive was installed\n\n");
    md.push_str("| Choice | Value | Why |\n|---|---|---|\n");
    md.push_str("| Partition style | GPT (`/GPT`) | Works on every UEFI machine and on legacy BIOS through Ventoy's own boot code; no 2 TB limit. |\n");
    md.push_str("| File system | exFAT (`/FS:EXFAT`) | Holds files over 4 GB (Windows ISOs) and is readable by Windows, Linux and macOS without drivers. |\n");
    md.push_str("| Secure Boot support | on (no `/NOSB`) | The drive boots on machines with Secure Boot enabled. The first boot on such a machine asks once to enrol Ventoy's key. |\n");
    md.push_str("| USB check | on (no `/NOUSBCheck`) | Ventoy itself refuses to install to anything that is not a USB disk. A second safety net behind Corvus Setup's own disk filter. |\n");
    md.push_str("| Volume name | `CORVUS` | Ventoy has no option for this, so Corvus Setup renames the partition after installing. Corvus.exe finds the drive by this name. |\n\n");

    md.push_str("## Global switches (`control`)\n\n");
    md.push_str("| Setting | Value | What it does | Why | Last changed |\n|---|---|---|---|---|\n");
    for s in CONTROL {
        md.push_str(&format!(
            "| `{}` | `{}` | {} | {} | {} |\n",
            s.key, s.value, s.what, s.why, s.since
        ));
    }
    match default_image {
        Some(img) => md.push_str(&format!(
            "| `VTOY_DEFAULT_IMAGE` | `{img}` | The entry that is highlighted when the menu opens. | Corvus PE is the usual starting point. Nothing boots by itself: no timeout is set. | {} |\n",
            &note.when[..note.when.len().min(10)]
        )),
        None => md.push_str("| `VTOY_DEFAULT_IMAGE` | not set | The entry highlighted when the menu opens. | Corvus PE is not on the drive yet, so the first entry is highlighted. | — |\n"),
    }
    md.push_str("| `VTOY_MENU_TIMEOUT` | not set | Would boot the highlighted entry after N seconds. | Left off on purpose: a technician drive should never start wiping or imaging tools unattended. | 2026-11-14 |\n\n");

    md.push_str("## Look (`theme`)\n\n");
    md.push_str("| Setting | Value | What it does | Why | Last changed |\n|---|---|---|---|---|\n");
    for s in THEME {
        md.push_str(&format!(
            "| `{}` | `{}` | {} | {} | {} |\n",
            s.key, s.value, s.what, s.why, s.since
        ));
    }
    md.push_str("\nBackground picture: a PNG at `ventoy\\theme\\corvus\\background.png` (1920x1080 recommended; it is stretched to the screen). Replace that one file to change it.\n\n");

    md.push_str("## Menu names (`menu_alias`)\n\n");
    md.push_str("Ventoy lists files by file name. These entries swap the file name for a friendly one. An entry whose file is not on the drive does nothing until the file is added.\n\n");
    md.push_str("| File | Shown as |\n|---|---|\n");
    for i in images {
        md.push_str(&format!("| `{}` | {} |\n", i.path, i.alias));
    }
    for (dir, name) in DIR_ALIASES {
        md.push_str(&format!(
            "| `{}` (folder) | {} |\n",
            dir.trim_start_matches('/'),
            name
        ));
    }
    md.push_str("\nOrder: Ventoy sorts the menu by file name and has no setting to reorder it. To move an entry, rename its file.\n\n");

    md.push_str("## What Ventoy scans\n\n");
    md.push_str(&format!(
        "Ventoy looks through the whole drive for bootable images. An empty file called `.ventoyignore` in a folder makes it skip that folder. Corvus Setup puts one in: {}. That keeps thousands of tool files out of the scan (faster menu) and stops stray `.img`/`.iso` files inside tools from showing up as boot entries. `Boot\\` and `Installers\\` are scanned, so any ISO dropped into them appears in the menu.\n\n",
        IGNORED_DIRS
            .iter()
            .map(|d| format!("`{d}\\`"))
            .collect::<Vec<_>>()
            .join(", ")
    ));

    md.push_str(HISTORY_HEADING);
    md.push_str("\n\n");
    if let Some(prev) = previous {
        if let Some(pos) = prev.find(HISTORY_HEADING) {
            for line in prev[pos + HISTORY_HEADING.len()..].lines() {
                if line.starts_with("- ") {
                    md.push_str(line);
                    md.push('\n');
                }
            }
        }
    }
    md.push_str(&format!(
        "- {} — {} — Ventoy {}\n",
        note.when,
        note.mode,
        if note.ventoy_command.is_empty() {
            "not run (boot files untouched)".to_string()
        } else {
            format!("command: `{}`", note.ventoy_command)
        }
    ));
    md
}

/// Write all the generated files under `root`. Each failure is returned as a line of text
/// instead of stopping, so one read-only file cannot block the others.
pub fn write_all(root: &Path, note: &RunNote) -> (Vec<String>, Vec<String>) {
    let mut done = Vec::new();
    let mut errors = Vec::new();
    let mut put = |rel: &str, text: String| {
        let path = root.join(rel);
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        match fs::write(&path, text) {
            Ok(()) => done.push(format!("wrote {rel}")),
            Err(e) => errors.push(format!("could not write {rel}: {e}")),
        }
    };

    for dir in ["Boot", "Installers\\Windows", "Installers\\Linux"] {
        let _ = fs::create_dir_all(root.join(dir));
    }

    let (images, warn) = load_boot_menu(root);
    let pe_present = root.join(PE_IMAGE).is_file();
    let default_image = pe_present.then(|| format!("/{PE_IMAGE}"));

    put(
        "ventoy\\ventoy.json",
        ventoy_json(&images, default_image.as_deref()),
    );
    let previous = fs::read_to_string(root.join("ventoy").join("ventoy-log.md")).ok();
    put(
        "ventoy\\ventoy-log.md",
        ventoy_log_md(&images, default_image.as_deref(), note, previous.as_deref()),
    );
    put("autorun.inf", autorun_inf());
    for dir in IGNORED_DIRS {
        if root.join(dir).is_dir() {
            put(&format!("{dir}\\.ventoyignore"), String::new());
        }
    }
    errors.extend(warn);
    (done, errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn images() -> Vec<BootImage> {
        vec![
            BootImage {
                path: "Boot/Lockpick.wim".into(),
                alias: "Jayro's \"Lockpick\"".into(),
            },
            BootImage {
                path: "Boot/CorvusPE.iso".into(),
                alias: "Corvus PE".into(),
            },
        ]
    }

    fn note() -> RunNote<'static> {
        RunNote {
            when: "2026-11-14 20:00:00",
            mode: "Fresh install",
            ventoy_version: "1.1.18",
            ventoy_command: "Ventoy2Disk.exe VTOYCLI /I /PhyDrive:3 /GPT /FS:EXFAT",
        }
    }

    #[test]
    fn json_escaping() {
        assert_eq!(json_str(r#"a"b\c"#), r#""a\"b\\c""#);
        assert_eq!(json_str("tab\there"), r#""tab\there""#);
        assert_eq!(json_str("烏"), "\"烏\"");
    }

    #[test]
    fn ventoy_json_shape() {
        let j = ventoy_json(&images(), Some("/Boot/CorvusPE.iso"));
        assert!(j.contains(r#"{ "VTOY_DEFAULT_MENU_MODE": "0" }"#));
        assert!(j.contains(r#"{ "VTOY_DEFAULT_IMAGE": "/Boot/CorvusPE.iso" }"#));
        assert!(j.contains(r#""file": "/ventoy/theme/corvus/theme.txt""#));
        assert!(j.contains(r#"{ "image": "/Boot/Lockpick.wim", "alias": "Jayro's \"Lockpick\"" }"#));
        assert!(j.contains(r#"{ "dir": "/Boot", "alias": "[Boot tools]" }"#));
        assert!(!j.contains("VTOY_MENU_TIMEOUT"), "never auto-boot");
        assert!(
            !j.contains("VTOY_DEFAULT_SEARCH_ROOT"),
            "Installers must stay visible"
        );
        // Balanced and comma-clean enough for a strict parser: no ",\n  ]" or ",\n  }".
        assert!(!j.contains(",\n  ]") && !j.contains(",\n  }"));
        assert_eq!(j.matches('{').count(), j.matches('}').count());
        assert_eq!(j.matches('[').count(), j.matches(']').count());

        let j = ventoy_json(&images(), None);
        assert!(!j.contains("VTOY_DEFAULT_IMAGE"));
    }

    #[test]
    fn dir_aliases_have_no_trailing_slash() {
        for (dir, _) in DIR_ALIASES {
            assert!(dir.starts_with('/') && !dir.ends_with('/'), "{dir}");
        }
    }

    #[test]
    fn log_explains_every_setting_and_keeps_history() {
        let first = ventoy_log_md(&images(), None, &note(), None);
        for s in CONTROL.iter().chain(THEME) {
            assert!(first.contains(s.key), "{} missing from log", s.key);
            assert!(!s.what.is_empty() && !s.why.is_empty());
        }
        assert!(first.contains("/I /PhyDrive:3 /GPT /FS:EXFAT"));

        let second_note = RunNote {
            when: "2026-12-01 09:00:00",
            mode: "Payload only",
            ventoy_version: "1.1.18",
            ventoy_command: "",
        };
        let second = ventoy_log_md(&images(), None, &second_note, Some(&first));
        let history = &second[second.find(HISTORY_HEADING).unwrap()..];
        assert_eq!(history.lines().filter(|l| l.starts_with("- ")).count(), 2);
        assert!(history.contains("2026-11-14 20:00:00 — Fresh install"));
        assert!(history.contains("2026-12-01 09:00:00 — Payload only — Ventoy not run"));
    }

    #[test]
    fn write_all_creates_everything_and_survives_missing_boot_menu() {
        let d = tempfile::tempdir().unwrap();
        fs::create_dir_all(d.path().join("Tools")).unwrap();
        fs::create_dir_all(d.path().join("Launcher")).unwrap();
        let (done, errors) = write_all(d.path(), &note());
        assert!(done.iter().any(|l| l.contains("ventoy.json")));
        assert_eq!(errors.len(), 1, "only the boot-menu warning: {errors:?}");
        for f in [
            "ventoy/ventoy.json",
            "ventoy/ventoy-log.md",
            "autorun.inf",
            "Tools/.ventoyignore",
            "Launcher/.ventoyignore",
            "Boot",
            "Installers/Windows",
            "Installers/Linux",
        ] {
            assert!(d.path().join(f).exists(), "{f}");
        }
        assert!(
            !d.path().join("Branding/.ventoyignore").exists(),
            "no folder, no marker"
        );
        let j = fs::read_to_string(d.path().join("ventoy/ventoy.json")).unwrap();
        assert!(
            j.contains("/Boot/CorvusPE.iso"),
            "PE alias is always present"
        );
        assert!(
            !j.contains("VTOY_DEFAULT_IMAGE"),
            "but not default while the iso is absent"
        );
    }

    #[test]
    fn boot_menu_file_is_read() {
        let d = tempfile::tempdir().unwrap();
        fs::create_dir_all(d.path().join("Launcher")).unwrap();
        fs::write(
            d.path().join("Launcher/boot-menu.toml"),
            "[[image]]\npath = \"Boot/Lockpick.wim\"\nalias = \"Jayro's Lockpick\"\n",
        )
        .unwrap();
        let (images, warn) = load_boot_menu(d.path());
        assert!(warn.is_none());
        assert_eq!(images.len(), 2);
        assert_eq!(images[0].alias, PE_ALIAS);
        assert_eq!(images[1].alias, "Jayro's Lockpick");
    }
}
