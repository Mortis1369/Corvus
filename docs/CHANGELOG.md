# Corvus changelog

All dates are the day the work was done. "VERIFY" notes record where the brief was checked against
real docs/source and what was found.

## 2026-11-14 — Milestone 3: `Corvus.exe`

- New crate `crates/corvus` → single 4.9 MB `Corvus.exe`, no external DLLs, icon and version block
  embedded via `build.rs` + `winresource`.
- Modules: `app` (window, sidebar, tiles, power bar, toasts, resize handle), `launch` (ShellExecute
  launch + power actions), `icons` (exe icon extraction via Win32), `credits` (credits page).
- Verified against `S:\Corvus\test-drive` built by `build-tools/make-test-drive.ps1`: 17 tools load,
  5 deliberately absent tools show as greyed "missing" tiles, a wrong `sha256` shows as "changed",
  three malformed `[[tool]]` entries are skipped and listed on the Problems page, and the launcher
  never crashes. PE path tested with `CORVUS_FORCE_PE=1` (inward glow, PE-only tools enabled,
  live-only greyed, two power buttons).
- Hash verification runs on a background thread after the first frame; exe icons are extracted
  three per frame so a slow stick never stalls the UI.
- Power bar buttons need a second click within 4 s ("Confirm …") — the brief says forced, and they
  are, but one misclick on a client machine shouldn't power it off.
- **VERIFY (power in PE):** WinPE has no `shutdown.exe`; PE uses `wpeutil shutdown` / `wpeutil reboot`
  and hides "Restart to UEFI" and "Advanced Startup", which have no PE equivalent. To be confirmed
  on the real Corvus PE build. On live Windows `shutdown /r /fw` needs admin, so that one button
  runs `shutdown.exe` with the `runas` verb (UAC prompt); the launcher itself stays unelevated.
- **VERIFY (egui 0.37 API drift):** `eframe::App::update(ctx)` is now `ui(&mut Ui)`;
  `TopBottomPanel`/`SidePanel` merged into `egui::Panel::{top,bottom,left,right}` with `exact_size`;
  `Context::screen_rect()` is `content_rect()`; `Id::new` is deprecated for `Id::unique`.
- Pulled forward from Milestone 6: `build-tools/make-icons` (PNG or SVG → `.ico` + PNG set,
  normalised to a light glyph on transparent). `branding/karasu.png` is Aiden's Kanji.png; drop the
  real `karasu.svg` beside it and re-run `cargo run -p make-icons`.

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
