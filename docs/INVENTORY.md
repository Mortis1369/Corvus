# Corvus payload inventory — Milestone 1

Source surveyed: `S:\Corvus\PortableApps` (MediCat PortableApps copy, 152 folders, ~30 GB of which 25 GB is SDI Origin driver packs).
Date: 2026-11-14. Nothing has been copied yet.

Version column = file version read from the exe. "Outdated" = build is 2020–2022 era; the exact current version is verified against the vendor at fetch time (Milestone 4), not guessed here.

## A. Keep list — status

### Diagnostics

| Tool | Found in source | Version | Status | Action |
|---|---|---|---|---|
| AIDA64 | `AIDA64Portable\App\AIDA64Extreme\aida64.exe` (+ Business, Engineer, Network Audit editions) | 6.60.5900 | Outdated. Multi-edition bundle of a paid product, no key files found — see Q3 | Replace with official AIDA64 Extreme portable ZIP (aida64.com); trial unless Aiden has a licence |
| Prime95 | `Prime95\prime95.exe` | 30.8 b1 | Outdated | Fetch official ZIP (mersenne.org) |
| OCCT | — | — | **Missing** | Fetch from ocbase.com. Licence: free edition is personal-use only — see Q5 |
| TestMem5 + anta777 config | — | — | **Missing** | Fetch TM5 + anta777 cfg from their GitHub releases (VERIFY exact repos) |
| GPU-Z | `GPU-ZPortable\App\GPU-Z\GPU-Z.exe` | 2.43.0 | Outdated | Fetch official exe (techpowerup.com); single-exe portable |
| CPU-Z | `CPU-ZPortable\App\CPU-Z\cpuz_x64.exe` | 1.98 | Outdated | Fetch official portable ZIP (cpuid.com) |
| FurMark 2 | — | — | **Missing** | Fetch official portable ZIP (geeks3d.com) |
| HWiNFO | `HWiNFOPortable\App\HWiNFO\HWiNFO64.exe` | 7.14-4610 | Outdated | Fetch official portable ZIP (hwinfo.com) |
| CrystalDiskInfo | `CrystalDiskInfoPortable\App\CrystalDiskInfo\DiskInfo64.exe` | 8.13.0 | Outdated | Fetch official portable ZIP (crystalmark.info) |
| CrystalDiskMark | `CrystalDiskMarkPortable\App\CrystalDiskMark\DiskMark64.exe` | 8.0.4 | Outdated | Fetch official portable ZIP (crystalmark.info) |
| Victoria | `Victoria536\Victoria.exe` | 5.3.6 | OK (recent-ish) | Keep; check hdd.by for newer |
| WhoCrashed / BlueScreenView | `BlueScreen View\BlueScreenView.exe` | 1.55 | OK (BlueScreenView; NirSoft but not a password tool) | Keep BlueScreenView. WhoCrashed free edition is home-use only — see Q5 |
| Fan Control | `FanControl\FanControl.exe` | build 285 | Present. Needs .NET Desktop Runtime on the host; `mode = "live"` | Keep; check GitHub (Rem0o/FanControl.Releases) for newer |

### System / cleanup

| Tool | Found in source | Version | Status | Action |
|---|---|---|---|---|
| Sysinternals Autoruns | `AutorunsPortable\App\Autoruns\Autoruns64.exe` | 14.06 | Outdated | Replace all three with the official **Sysinternals Suite** ZIP (download.sysinternals.com) — also gives RAMMap, VMMap, Disk2vhd etc. |
| Sysinternals Process Explorer | `ProcessExplorerPortable\App\ProcessExplorer\procexp64.exe` | 16.43 | Outdated | As above |
| Sysinternals Process Monitor | — | — | **Missing** | As above |
| BCUninstaller | — | — | **Missing** | Fetch portable ZIP from GitHub (Klocman/Bulk-Crap-Uninstaller) |
| DDU | `DDU v18.0.3.3\Display Driver Uninstaller.exe` | 18.0.3.3 | Outdated | Fetch official (wagnardsoft.com) |
| WizTree | — | — | **Missing** | Fetch portable ZIP (diskanalyzer.com). Licence: free for home use only — see Q5. WinDirStat 1.1.2 is present as a free alternative (WinDirStat 2.x now has an official portable) |
| Everything | — | — | **Missing** | Fetch official portable ZIP (voidtools.com) |
| Wise Disk Cleaner | `WiseDiskCleanerPortable\App\WiseDiskCleaner\WiseDiskCleaner.exe` | 10.7.2 | Outdated | Fetch official portable (wisecleaner.com) |
| Wise Registry Cleaner | `WiseRegistryCleanerPortable\App\WiseRegistryCleaner\WiseRegCleaner.exe` | 10.5.1 | Outdated | Fetch official portable (wisecleaner.com). Kept per Aiden |

### Malware second opinion

| Tool | Found in source | Version | Status | Action |
|---|---|---|---|---|
| AdwCleaner | — | — | **Missing** | Fetch single exe (malwarebytes.com/adwcleaner). Licence for business use: VERIFY |
| Emsisoft Emergency Kit | — | — | **Missing** | Fetch (emsisoft.com). Licence: free for private use; commercial needs a licence — see Q5 |

Present but *not* on the keep list: TDSSKiller 3.1.0.28 (discontinued), Dr.Web CureIt! (218 MB, home-use only), McAfee Stinger 12.2, SUPERAntiSpyware 10, HijackThis Fork 2.10, Panda Cloud Cleaner 1.4. Dropped unless Aiden says otherwise.

### Drivers

| Tool | Found in source | Version | Status | Action |
|---|---|---|---|---|
| Snappy Driver Installer **Origin** | `SnappyDriverInstaller\SDIO_x64_R887.exe` (+ R739, R830, XP builds) | 2.0.4.887 | Present, confirmed Origin (not original SDI). Driver packs: **42 archives, 25 GB** | Keep R887 exes only. Driver pack trimming is Q2 |

Driver packs present: current (`_26xxx`, 2026) set covers Chipset, LAN, WLAN, Bluetooth, Sound (Conexant/Intel/Waves/HDMI/Realtek), Touchpad (Elan/Synaptics), Video (Intel DCH 30/31/32, nVIDIA DCH 5x + NT, AMD DCH 31/32), MassStorage, Misc, Monitor, USB3, Vendor, Battery, Media, System, Virtual. Also 11 superseded 2021 packs (`_21xxx`, ~1.7 GB) which SDIO ignores in favour of the newer ones.

### Disk / data

| Tool | Found in source | Version | Status | Action |
|---|---|---|---|---|
| DiskGenius Free | — | — | **Missing** | Fetch from diskgenius.com (VERIFY a portable ZIP exists; otherwise extract installer) |
| TestDisk / PhotoRec | — | — | **Missing** | Fetch official ZIP (cgsecurity.org) — portable by nature |
| FreeFileSync portable | — | — | **Missing** | freefilesync.org ships one installer; its "Portable" install type produces the portable folder. Run once into a staging dir, never the "Local" install |
| VeraCrypt portable | `VeraCryptPortable\App\VeraCrypt\VeraCrypt-x64.exe` | 1.24-Update7 (2020) | Outdated | Fetch official "VeraCrypt Portable" package (veracrypt.fr) |
| 7-Zip portable | `7-ZipPortable\App\7-Zip64\7zFM.exe` | 21.06 | Outdated (and PortableApps.com wrapper) | 7-zip.org ships no portable GUI zip; extract the official x64 installer (a 7z SFX) into the tool folder |

### Browsers

| Tool | Found in source | Version | Status | Action |
|---|---|---|---|---|
| Tor Browser (the only portable browser present) | `TorBrowser\Browser\firefox.exe` | 11.x / Firefox 91.4 ESR (Dec 2021) | Present but 4 years stale and an odd fit for a repair bench | See Q4 |

### Deployment

| Tool | Found | Version | Status | Action |
|---|---|---|---|---|
| Crowbar (release) | `S:\Corvus\Crowbar.exe` | 0.1.0, 8.6 MB | **This is the one to ship** (Aiden, 2026-11-14) | Copy to `Tools\Deployment\Crowbar\` |
| Crowbar (prerelease) | `PortableApps\Crowbar\Crowbar.exe` + rmskins, logs, cache | 0.1.0, 140 MB | Aiden's private prerelease | **Do not stage** |

### Boot side (Ventoy menu) — none of these live in PortableApps

| Image | Status | Action |
|---|---|---|
| lockpick.wim | Not in source | **Q1: where is it on the MediCat drive?** |
| BootIt | Not in source | **Q1** |
| memtest86 (UEFI) | Not in source | Fetch official free edition (memtest86.com) |
| memtest86+ | Not in source | Fetch official (memtest.org) |
| Rescuezilla | Not in source | Fetch official ISO (GitHub rescuezilla/rescuezilla) |
| GParted Live | Not in source | Fetch official ISO (gparted.org) |
| ShredOS | Not in source | Fetch official img (GitHub PartialVolume/shredos.x86_64) |
| Corvus PE | Not ready | Optional input; skipped if absent |
| Macrium Reflect rescue media | Not in source | See Q6 |

## B. Explicitly excluded (brief §5) — present in source, will not be carried over

| Folder | Reason |
|---|---|
| `ProcessHackerPortable` | Process Hacker — excluded by name |
| `Wireless Key View` | NirSoft password/key tool |
| `RecoverKeysPortable` | Licence-key recovery tool |
| `UEFI BIOS Updater 1.69.17.2` | Excluded by name |
| `Software Lister.bat` | Excluded by name |
| `Macrorit Partition Expert 5.6.1 Technician Edition (x64)` | Technician repack; replaced by DiskGenius Free |
| `EaseUS Todo PCTrans Professional 11.5` | Pro repack, excluded by name |
| `Malware.Hunter.Pro.1.111.0.703.Portable` | Pro repack |
| `Auslogics.Disk.Defrag.Pro.*`, `Auslogics.Driver.Updater.*` | Pro repacks |
| `Driver Talent Pro 6.5.60.172`, `Driver Booster` | Pro repack / adware-grade driver updater |
| `RecuvaPortable`, `Defraggler`, `SpeccyPro` | Wrapped as "Recuva/Defraggler/Speccy **Professional** Portable" — repacks |
| `PowerISO 7.9`, `UltraISOPortable`, `DeepL Pro v2.4.0`, `R-Drive`, `RS RAID Retrieve`, `Disk Drill`, `SSD Fresh Plus 2021`, `AIDA64 Business/Engineer` | Paid software with no licence on file — treated as repacks |
| `uefipart.exe` | Named in the brief; **not found** in source (nothing to exclude) |

## C. Not on the keep list — dropped by default

Games (3D Pinball, Kobo Deluxe, Mines, Solitaire, Sudoku, TileWorld), chat/media (Discord, Telegram, FreeTube, qBittorrent), office/graphics (AbiWord, Paint.NET, XnView, IrfanView, Lightscreen, PNotes, PngOptimizer), network (Wireshark 3.4, OpenVPN, AnyDesk 6.3, KiTTY), BIOS flashers (ASUS WinFlash, Dell flash utility, UEFITool), imaging (BurnAware, IsoBurner, balenaEtcher, WinToHDD/WinToUSB/WinUSB, VHD tools), misc cleaners (CCleaner, BleachBit, JkDefrag, UltraDefrag), registry tools (RegOwnershipEx, RegistryWorkshop (empty), RegistryManager, Regshot, RegistryLoaderPE), and the rest.

**Worth a second look** (live-safe, small, genuinely useful on a bench; Aiden picks): Notepad++ 8.1, Explorer++ 1.3.5, Q-Dir 10.4, SumatraPDF 3.3, HxD 2.4, Rufus 3.17, PuTTY 0.76, WinSCP 5.19, RAMMap/VMMap (come free with Sysinternals Suite), Dead Pixel Tester 3.0, Unstoppable Copier 5.2, Windows Media Creation Tools (Win10/11), Error Lookup 2.3. All would be refetched from vendors anyway since these copies are 2021-2022.

## D. Open questions for Aiden (blocking Milestone 4, not Milestones 2–3)

1. **Boot images:** where do `lockpick.wim` and BootIt live on the current MediCat drive?
2. **SDI Origin driver packs — 25 GB.** Options: (a) keep everything current, drop the 11 superseded 2021 packs (~23 GB); (b) laptop-repair core only: Chipset, LAN, WLAN, Bluetooth, Sound, Touchpad, MassStorage, USB3, Monitor, Misc, Vendor (~8 GB) and let SDIO download GPU packs on demand; (c) everything as-is. Recommendation: (a).
3. **AIDA64:** the source bundle has four editions of a paid product with no key files, so it is either trial-mode or pre-patched. Do you have a licence? If not, ship the official Extreme trial ZIP or drop it (HWiNFO covers most of it).
4. **Browser:** only Tor Browser (2021) is present. Mozilla/Google/Brave ship no official portable builds. Official-portable options: current **Tor Browser** (torproject.org), **LibreWolf** portable (librewolf.net, Firefox-based), or **ungoogled-chromium** portable ZIP (GitHub). Recommendation: LibreWolf, or skip browsers entirely.
5. **Shop-use licences:** OCCT (free = personal only), WhoCrashed (free = home only), WizTree (free = home only), Emsisoft Emergency Kit (free = private use), Dr.Web CureIt (home only). None are AV-flagged; this is a terms question. Your call per tool — BlueScreenView, WinDirStat 2 and TestDisk are clean alternatives for three of them.
6. **Macrium Reflect:** installed copy is **Reflect 8 Free, v8.0.7783**, licensee "Free User". Copying `C:\Program Files\Macrium\Reflect` to the stick is not a form Macrium supports: Reflect needs its service, VSS helper and mount drivers registered on the host, so it would not run as a portable exe. Its supported "portable" form is the **Rescue Media** (a bootable WinPE ISO built by Reflect's own Rescue Media Builder) — that drops straight into `Boot\` and Ventoy boots it. Reflect Free was discontinued in Jan 2024 and is no longer downloadable, so building that rescue ISO from this install now is worth doing. Recommendation: build `Boot\MacriumRescue.iso` with Rescue Media Builder, skip the live-side copy.
7. **Extras** from section C: any you want kept?

## E. Decisions (Aiden, 2026-11-14)

1. Boot images: `M:\Boot_Repair\BootIt_Bare_Metal\BootIt_Bare_Metal.iso` (3.0 MB, 2021) and `M:\Password_Removal\Jayro's_Lockpick\Lockpick.wim` (258 MB, 2021). M: is the current MediCat drive.
2. SDI Origin: drop the 11 superseded 2021 driver packs; keep the 2026 set.
3. AIDA64: no licence owned; ship official AIDA64 Extreme portable (trial).
4. Browser: ungoogled-chromium official portable ZIP.
5. Shop-use licence terms: no change requested; tools stay as the brief lists them. Terms noted per tool in tools.toml/credits.
6. Macrium: build a Rescue Media ISO from the installed Reflect 8 Free into Boot\; no live-side copy.
7. Extras: keep all of section C's "worth a second look" list (Notepad++, Explorer++, Q-Dir, SumatraPDF, HxD, Rufus, PuTTY, WinSCP, RAMMap/VMMap, Dead Pixel Tester, Unstoppable Copier, Media Creation Tools, Error Lookup), refetched from vendors.
