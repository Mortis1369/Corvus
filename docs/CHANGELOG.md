# Corvus changelog

All dates are the day the work was done. "VERIFY" notes record where the brief was checked against
real docs/source and what was found.

## 2026-11-15 — Milestone 5: first real run, disk filter changed

- **First real Fresh install (Aiden, 14.6 GB "Generic Flash Disk"):** Ventoy 1.1.18 installed
  through the CLI without error (GPT, exFAT), the volume was found straight away and renamed from
  `Ventoy` to `CORVUS`. The run then stopped at the free-space check: 30.2 GB needed, 14.6 GB
  free. Nothing was copied. **Known gap:** that check runs after the wipe, not before it.
- **Disk filter is now "attached by USB" only.** Before: USB/SD/MMC bus, or anything with the
  removable-media flag. Now: USB bus and nothing else, so built-in card slots and hot-swap SATA
  bays are no longer offered. USB hard drives and SSDs are offered (they were never actually
  excluded by the old rule, only by the "disk Corvus Setup runs from" protection on this PC).
- **Warning for formatted USB hard drives / SSDs:** a disk without the removable-media flag that
  already has partitions (counted with `IOCTL_DISK_GET_DRIVE_LAYOUT_EX`, so partitions without a
  drive letter count too) shows a warning on the Choose screen and again inside the red erase
  box on the Confirm screen. Flash sticks and blank disks get no warning.
- **Tested:** 29 unit tests. **Not tested:** a real USB hard drive / SSD as the target. Ventoy's
  own USB check is still on and has not been seen accepting a non-removable USB disk yet.

### Later the same day — second real run (476.9 GB "Realtek RTL9210 NVME" USB enclosure)

- **Ventoy's USB check accepts a non-removable USB disk:** `VTOYCLI /I` succeeded in 11 s with
  the check on. Volume found and renamed to `CORVUS`.
- **The copy then lost both drives.** 2,090 files (4.8 GB) copied in about 3.5 minutes, then at
  20:56:17 Windows logged "Delayed Write Failed" on `F:` and an I/O retry on the project disk in
  the same second. All 3,922 remaining files failed with "path not found" within that second, the
  config files could not be written, and the run still reported "Finished". Both drives hang off
  the same Genesys Logic hub, and the target enclosure was on its USB 2.0 side (VID 05E3 PID
  0610), which matches the ~23 MB/s copy speed. Cause is hardware (hub reset or power), not code.
- **Fix: a vanished drive stops the run.** After any failed file the sync checks that the target
  root and the payload folder still exist, and also stops after 25 failures in a row. The run
  ends as "Stopped" with advice (redo Fresh install; use a USB 3 port on the PC, not a hub)
  instead of thousands of identical skips and a false "Finished".
- **Corvus PE is now a PhoenixPE build** (base image Windows 11 IoT Enterprise LTSC), not
  Win10XPE. Differs from the brief. The PE field now pre-fills from
  `D:\PhoenixPE-master\<release>\Output\*.iso` (newest wins; the old Win10XPE folder is still
  searched). The current ISO is an untouched base export: provisional, included for testing.
- **Tested:** 31 unit tests.

### Third run and follow-up changes

- Target moved to a port on a different root hub (`USB\ROOT_HUB30`, no longer behind the
  internal Genesys Logic hub the project disk uses). Fresh install copying at about 32 MB/s with
  no failures at the half-way mark.
- **Corvus PE is no longer chosen in the window.** The path field is gone. The ISO is a
  catalogue `[[boot]]` entry: `stage-payload` copies it from the PhoenixPE output folder to
  `payload/Boot/CorvusPE.iso`, and Corvus Setup installs it with the rest of the payload. The
  Sources card just shows "Corvus PE: included (size)" or that the payload has none.
  **Differs from the brief (§8.5),** which asked for a field.
- `stage-payload`: a boot image is re-copied when the source is newer, not only when its size
  differs, so a rebuilt ISO of the same size is picked up.
- **Size check before the erase.** The payload is measured at start-up; a Fresh install onto a
  disk smaller than payload + 512 MB is blocked on the Choose screen ("This disk is too
  small…"), and the engine checks again immediately before running Ventoy. The 14.6 GB stick
  from the first run would now be refused untouched.
- **Third run finished clean:** 6,012 files / 30.1 GB, 0 skipped, about 16 minutes. Drive verified
  read-only afterwards (label, `ventoy.json`, `autorun.inf`, ignore markers, no leftover parts).
- **Drive icon:** `autorun.inf` is written after the copy, and Explorer only reads it when the
  drive appears, so the crow icon showed after a replug (confirmed by Aiden). Setup now also
  asks Explorer to refresh the drive straight after writing it (`SHChangeNotify`; best effort,
  not yet seen working).
- **Support line:** a small "If Corvus saves you time, please consider supporting its
  development" line with GitHub Sponsors and Ko-fi links (mortis1369) at the bottom of the
  Choose screen and on a successful Finished screen. Links open the browser only when clicked.
- **Launcher support pop-out:** two small overlapping coins beside the Live/WinPE tag in
  `Corvus.exe`'s title strip. Clicking them opens a small pop-out with the same sentence and the
  two links (addresses also shown as text, since WinPE may have no browser). Closes on a click
  elsewhere or Escape. Still no network calls by the launcher itself.

## 2026-11-14 (evening) — Milestone 5, part 1: CorvusSetup.exe (not yet run against a real stick)

- **Relocation fix:** catalogue paths (`sevenzip`, `local`) may now be relative to the workspace
  root; the five `S:/Corvus/...` entries became `../...`. `make-test-drive.ps1` derives its
  defaults from its own location. The project no longer cares what drive letter it is mounted on.
- **Catalogue:** new `skip = "reason"` field (entry is left out and reported as skipped, not
  FAILED). DDU 18.1.6.2 staged from Aiden's download (it is a 7-Zip SFX, so it is unpacked, not
  run). HxD skipped (mh-nexus.de down). FreeFileSync skipped: the installer's Portable option is
  Donation Edition only and the source zip has no Windows build files (Aiden may revisit after
  checking the Donation Edition's redistribution terms). VeraCrypt 1.26.31 staged after Aiden ran
  its GUI extractor by hand. **46 launcher entries, 0 failed.**
- **Ventoy background:** Aiden's 1920x1080 art is now `branding/ventoy-background.png` and is
  staged as the theme background (not yet seen in a real Ventoy menu).
- **stage-payload** now also writes `payload/Launcher/boot-menu.toml` (file → Ventoy menu name,
  for every catalogue boot image whether staged or not) and stages `payload/Branding/` and
  `payload/ventoy/theme/corvus/`. Background: `branding/ventoy-background.png` if present, else
  the generated placeholder.
- **CorvusSetup.exe** (`crates/corvus-setup`, 5 MB): modules `disks`, `ventoy`, `sync`, `config`,
  `engine`, `app`. Three modes: Fresh install, Update, Payload only (no Ventoy, no admin).
  - Disk filter: USB/SD/MMC bus or removable only; never a disk holding Windows, a pagefile, or
    CorvusSetup's own exe/payload/Ventoy folder; fails closed if the Windows disk cannot be
    identified (except in PE). The disk is re-checked (same number, model, size) right before use.
  - Fresh install needs `CORVUS` typed exactly. Close is disabled while a job runs.
  - Self-elevates at start (UAC); declining leaves Payload only available.
  - Payload sync: new/changed only (size, or mtime differing by > 2 s), never deletes, never
    touches `Installers\`, writes through `.corvus-part` + rename, failed files logged and skipped,
    free-space check before the first byte.
  - Writes `ventoy.json`, `ventoy-log.md` (every setting: what, why, date; run history kept),
    `autorun.inf`, `.ventoyignore` in Tools/Launcher/Branding, `Launcher\setup-log.txt`.
  - Ventoy is found in `ventoy\` beside the exe (or `build-tools/downloads/ventoy-*` in dev),
    copied to `%TEMP%\corvus-setup\ventoy` and run from there.
- **VERIFY (Ventoy docs re-read 2026-11-14):** `.ventoyignore` confirmed (skips the folder and its
  subfolders). No menu-ordering key exists; order is by file name. `fonts` in the theme plugin is
  optional and is not written (no `.pf2` shipped). **Differs from ventoy-notes:**
  `VTOY_DEFAULT_SEARCH_ROOT` is *not* used, because it allows one root only and `Installers\`
  must stay bootable; `.ventoyignore` markers do the job instead. No menu timeout is set.
- **Tested:** 25 unit tests (sync, config, command lines, disk rule, whole job into a temp folder);
  the real exe run headless (`--headless-folder`) built a complete drive tree twice (second run
  copied 0 files); on this PC the scan correctly refused all three disks, including the USB disk
  the project itself lives on. **Not tested:** the Ventoy install/update path and the window's
  layout by eye.

## 2026-11-14 — Milestone 4: payload staging

- `build-tools/payload-catalogue.toml` is the single source of truth: 47 tool entries and 8 boot
  images, every one with its vendor page and a direct URL on the vendor's own host or GitHub
  releases (SourceForge only where the vendor hosts there: CrystalDiskInfo/Mark, GParted, WinSCP).
- `build-tools/stage-payload` downloads (via `curl.exe`), extracts (via 7-Zip; the freshly staged
  26.06 is preferred over the old bootstrap copy because newer archives use methods 21.06 lacks),
  trims (`delete`), hashes the launcher exe, auto-reads the version from the exe when the catalogue
  leaves it blank, and writes `payload/Launcher/tools.toml`, `credits/LICENSES.md` plus any licence
  files found in each tool, and `payload/STAGING-REPORT.md`. `--reuse` keeps staged folders so the
  catalogue can be regenerated without re-copying 25 GB. `--only a,b` stages a subset.
- Result: **44 launcher entries staged and verified** (the launcher loads them with 0 missing),
  7 boot images, 31 GB total of which 25.9 GB is SDI Origin's 2026 driver-pack set.
- **Needs a human click (4 tools + 1 image):** DDU (Cloudflare), FreeFileSync (portable mode is an
  installer option), VeraCrypt (GUI extractor), HxD (site refuses scripts), Macrium rescue ISO.
  Instructions in `docs/payload-manual-items.md`. Until then their tiles are greyed "missing".
- Trimmed on purpose: Sysinternals ZoomIt/RDCMan (330 MB), BCU's ARM64 runtime (213 MB),
  Notepad++ updater, 7-Zip's uninstaller, SDIO's 2021 duplicate driver packs.
- **VERIFY (Ventoy):** latest is 1.1.18 (2026-10-09); `ventoy-1.1.18-windows.zip` downloaded and its
  SHA-256 matches the release's `sha256.txt`. `VTOYCLI` syntax, `ventoy.json` keys (menu_alias,
  menu_class, control, theme) and boot-image support recorded in `docs/ventoy-notes.md`. There is
  **no label option** in the CLI, so CorvusSetup will set `CORVUS` afterwards.
- Placeholder Ventoy theme: `branding/ventoy-theme/theme.txt` plus a generated
  `branding/generated/ventoy-background.png` (dark, faint kanji) — swap the PNG for real art.
- Launcher: after showing the initially hidden window, send `ViewportCommand::Focus` so it lands in
  front of whatever the technician had open.

## 2026-11-14 — Launcher shape change (Aiden's alteration)

- `Corvus.exe` is now a fixed **9:16 portrait** panel pinned to the **top-right** of the primary
  monitor's work area (screen minus taskbar, read via `SPI_GETWORKAREA`). Body is 432x768 on a
  1080p screen and shrinks (keeping 9:16) on shorter work areas, never below 540 tall. Window is
  not resizable; the window starts hidden, is sized and positioned on the first frame, then shown,
  so it never flashes at the OS default position. Drag still works; double-click the title strip to
  snap it back to the corner.
- Layout reworked for the narrow body: category chips in a sideways-scrolling strip replace the
  sidebar, tiles flow into as many columns as fit (3 at 432 wide), the power bar is two rows
  (status, then four equal buttons with short labels "UEFI" / "Advanced"; full names in tooltips).
  Resize handle and maximise removed.
- Glow is now **white** falling off to nothing: composited shadow blur 36 / spread 3 / white at
  alpha 200 inside a 40 px transparent margin. PE's inward glow is a 24 px band easing white → black,
  and the content is inset by that band so nothing sits under the bright edge.

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
