# Payload items that need a human click

Four vendors block scripted downloads or ship a GUI-only extractor. Each one is a `kind = "manual"`
entry in `build-tools/payload-catalogue.toml`. Put the files where the table says, then run

```
cargo run --release -p stage-payload -- --only ddu,freefilesync,veracrypt,hxd --tools-only
cargo run --release -p stage-payload -- --reuse --tools-only      # regenerates tools.toml with everything
```

| Tool | Get it from | What to do | Put the result in |
|---|---|---|---|
| **DDU** 18.1.6.1 | https://www.wagnardsoft.com/display-driver-uninstaller-ddu → "DOWNLOAD & SUPPORT" (forum, Cloudflare-checked) | Download `DDU v18.1.6.1.exe`. It is a self-extractor: run it and let it unpack to a folder. | `build-tools\downloads\manual\ddu\` so that `Display Driver Uninstaller.exe` sits directly inside |
| **FreeFileSync** 14.13 | https://freefilesync.org/download.php (`FreeFileSync_14.13_Windows_Setup.exe`) | Run the installer, choose installation type **Portable**, browse to the folder on the right. Do not pick *Local*; that is the path with bundled offers. | `build-tools\downloads\manual\freefilesync\` with `FreeFileSync.exe` directly inside |
| **VeraCrypt** 1.26.31 | https://github.com/veracrypt/VeraCrypt/releases → `VeraCrypt.Portable.1.26.31.exe` | Run it, accept the licence, it asks where to extract. | `build-tools\downloads\manual\veracrypt\` with `VeraCrypt-x64.exe` directly inside |
| **HxD** | https://mh-nexus.de/en/hxd/ → "Portable" download | Extract the zip. | `build-tools\downloads\manual\hxd\` with `HxD.exe` directly inside |

Also optional, Boot side:

| Image | What to do | Put it at |
|---|---|---|
| **Macrium Reflect Rescue** | In Macrium Reflect 8 Free on this PC: *Other Tasks → Create Rescue Media*, choose ISO output. | `S:\Corvus\MacriumRescue.iso` (the catalogue copies it into `Boot\`) |
| **Corvus PE** | When the Win10XPE build is done. | `S:\Corvus\CorvusPE.iso` (added to the catalogue when it exists) |

Until these are in place the staging run reports them as FAILED and carries on; the launcher simply
shows their tiles greyed out as "missing".
