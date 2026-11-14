# Ventoy facts verified for CorvusSetup (2026-11-14)

Source: ventoy.net documentation pages fetched 2026-11-14, and the GitHub releases API.

## Release

- Latest: **Ventoy 1.1.18** (2026-10-09). Windows package `ventoy-1.1.18-windows.zip` (15 MB), with
  `sha256.txt` alongside in the same GitHub release. Install/update binary inside: `Ventoy2Disk.exe`.

## Command line (`doc_windows_cli.html`)

Supported since 1.0.86. Parameters are case-insensitive. Must be run elevated.

```
Ventoy2Disk.exe VTOYCLI CMD DISK [Options]
  CMD      /I install (wipes the disk)   |   /U update (keeps data)
  DISK     /Drive:F:   or   /PhyDrive:N
Options
  /GPT         GPT partition style (default is MBR)
  /NOSB        do NOT enable Secure Boot support (default: enabled, for both /I and /U)
  /NOUSBCheck  skip the "is this a USB disk" check (default: refuses non-USB — keep the check on)
  /R:XXX       reserve XXX MB at the end of the disk (install only)
  /FS:XXX      filesystem for install: NTFS / EXFAT / FAT32 ...
  /NonDest     non-destructive install
```

Result files are written beside `Ventoy2Disk.exe`:

| File | Meaning |
|---|---|
| `cli_log.txt` | full log of the run |
| `cli_percent.txt` | progress percentage |
| `cli_done.txt` | appears when finished; content `0` = success, `1` = failure |

**No volume-label option exists.** The data partition is labelled by Ventoy's default ("Ventoy");
CorvusSetup sets `CORVUS` afterwards with `SetVolumeLabelW`.

Corvus defaults (brief §8): `/I /PhyDrive:N /GPT /FS:EXFAT` — Secure Boot support stays on (no `/NOSB`),
USB check stays on (no `/NOUSBCheck`). Update: `/U /PhyDrive:N`.

## `ventoy.json` (`/ventoy/ventoy.json`, UTF-8)

### menu_alias (`plugin_menualias.html`)
```json
{ "menu_alias": [
    { "image": "/Boot/Lockpick.wim", "alias": "Jayro's Lockpick" },
    { "dir":   "/Boot",              "alias": "[Boot tools]" }
] }
```
`image` is the full path from the drive root; `dir` must not end in a slash. Fuzzy path matching supported.

### menu_class (`plugin_menuclass.html`)
`{ "key": "substring-of-filename", "class": "name" }` or `{ "dir": "/Boot", "class": "name" }` —
selects an icon class from the theme. Optional for Corvus.

### control (`plugin_control.html`) — the ones Corvus uses
| Key | Value | Why |
|---|---|---|
| `VTOY_DEFAULT_SEARCH_ROOT` | `/Boot` | only list images under Boot\ (keeps Installers\ ISOs out of the menu unless wanted — decide with Aiden) |
| `VTOY_MENU_TIMEOUT` | e.g. `30` | auto-boot default after N seconds; omit for no timeout |
| `VTOY_DEFAULT_IMAGE` | `/Boot/CorvusPE.iso` | pre-selected entry |
| `VTOY_DEFAULT_MENU_MODE` | `0` | list view |
| `VTOY_FILT_DOT_UNDERSCORE_FILE` | `1` | hide macOS `._` junk |
| `VTOY_WIN11_BYPASS_CHECK` | `1` | Win11 installer ISOs skip TPM/CPU checks |
| `VTOY_SECURE_BOOT_POLICY` | `0` | default |

### theme (`plugin_theme.html`)
```json
{ "theme": {
    "file": "/ventoy/theme/corvus/theme.txt",
    "gfxmode": "max",
    "display_mode": "GUI",
    "ventoy_left": "5%", "ventoy_top": "95%", "ventoy_color": "#808080",
    "fonts": [ "/ventoy/theme/corvus/unicode.pf2" ]
} }
```
`theme.txt` is a standard GRUB2 theme file. Backgrounds: `desktop-image: "background.png"` in
`theme.txt` — swapping the picture is a one-file replacement, as the brief wants.

## Boot-side support (`isolist.html`)
- `memtest86-usb.img` (MemTest86 Free, UEFI) — listed as supported; Ventoy has an optimisation for it.
- Memtest86+ ISO — supported (memdisk mode on legacy).
- Rescuezilla, GParted Live, ShredOS ISOs — standard Linux ISOs, supported.
- `.wim` files (Lockpick) — Ventoy boots WIM files directly (wimboot).
