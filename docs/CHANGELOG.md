# Corvus changelog

All dates are the day the work was done. "VERIFY" notes record where the brief was checked against
real docs/source and what was found.

## 2026-11-14 — Milestone 2: `corvus-common`

- New crate `crates/corvus-common` with modules `manifest`, `pe`, `drive`, `theme`, `logging`, `hash`.
  28 unit tests, all passing (`cargo test -p corvus-common`).
- `manifest`: `tools.toml` parsed **per `[[tool]]` entry** so one bad entry never sinks the catalogue.
  Rejects absolute paths, `..`, empty required fields, malformed `sha256`, duplicate ids; unknown keys
  are ignored for forward compatibility. Added two optional fields beyond the brief's schema:
  `license` (for the credits page) and `elevate` (launch via UAC; the launcher itself never elevates),
  plus free-text `notes`.
- `pe`: `HKLM\SYSTEM\CurrentControlSet\Control\MiniNT` probe via `windows-sys`; `CORVUS_FORCE_PE=1|0`
  env override for desktop testing.
- `drive`: root from `..\` of the `Launcher\` folder, fallback scan of mounted volumes for label `CORVUS`.
- `theme`: shared palette, `GlowStyle::{Composited, Inward}`, `window_frame()`, `paint_inward_glow()`,
  `CORVUS_GLOW=inward|composited` override for testing the PE path on a desktop.
- **VERIFY (egui):** pinned `egui`/`eframe` 0.37.0. The brief's `Frame`/`Shadow` snippet matches the
  0.37 API exactly (`Shadow { offset: [i8; 2], blur: u8, spread: u8, color }`,
  `Frame::new().fill().corner_radius().outer_margin().shadow()`). One deviation:
  `Color32::from_rgba_unmultiplied` is not `const`, so the glow colour is a function, not a constant.
- **VERIFY (egui fonts):** egui 0.37's default feature set is empty, so `default_fonts` must be enabled
  explicitly or no text renders. Done in the workspace `Cargo.toml`.
- Workspace release profile: `opt-level = "z"`, LTO, `panic = "abort"`, stripped — small single exes.

## 2026-11-14 — Milestone 1: inventory

- `docs/INVENTORY.md`: full survey of `S:\Corvus\PortableApps` against the keep list, excluded list,
  and Aiden's decisions (see section E there).
- Repo initialised.
