//! stage-payload: turns `build-tools/payload-catalogue.toml` into `payload/`.
//!
//! For every `[[tool]]`: fetch the official download (cached in `build-tools/downloads/`),
//! extract it with 7-Zip, drop it into `payload/Tools/<Category>/<Name>/`, hash the launcher
//! exe, and emit a `[[tool]]` entry for `payload/Launcher/tools.toml`. `[[boot]]` entries land
//! in `payload/Boot/`. A per-item report goes to `payload/STAGING-REPORT.md`.
//!
//! Nothing here is clever: downloads are `curl.exe` (shipped with Windows), extraction is
//! `7z.exe` (path in `[settings]`), and a failure in one item never stops the others.
//!
//! Usage:
//!   cargo run -p stage-payload                     # everything
//!   cargo run -p stage-payload -- --only hwinfo,cpuz
//!   cargo run -p stage-payload -- --tools-only | --boot-only
//!   cargo run -p stage-payload -- --no-download    # use the cache, fail if a file is missing
//!
//! Kinds:
//!   archive  zip / 7z / self-extracting exe → 7z.exe x, single top folder stripped automatically
//!   exe      the download *is* the program; copied to <dest>/<exe>
//!   local    copy a file or folder from `local` (e.g. Crowbar.exe, SDIO driver packs)
//!   manual   expects a pre-extracted folder at downloads/manual/<id>/ (sites that block scripts)
//!   alias    no files; a launcher entry pointing at another tool's folder (`of`), e.g. Procmon in the Sysinternals Suite

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

const BROWSER_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36";

// ---- catalogue schema ------------------------------------------------------------------------

#[derive(Deserialize)]
struct Catalogue {
    settings: Settings,
    #[serde(default)]
    tool: Vec<Entry>,
    #[serde(default)]
    boot: Vec<BootEntry>,
}

#[derive(Deserialize)]
struct Settings {
    downloads: String,
    payload: String,
    sevenzip: String,
}

#[derive(Deserialize, Clone, Default)]
#[serde(default)]
struct Entry {
    id: String,
    name: String,
    category: String,
    version: String,
    license: String,
    source: String,
    url: String,
    /// Form body for sites that hand out the file after a POST (TechPowerUp). The tool POSTs,
    /// reads the `Location` header, then GETs that.
    post: String,
    /// Cache file name; default is the last URL segment.
    file: String,
    /// archive | exe | local | manual | alias
    kind: String,
    /// Take only this sub-folder of the extracted archive.
    subdir: String,
    /// Launcher target, relative to the tool folder.
    exe: String,
    /// Folder under payload/. Default Tools/<category>/<name>.
    dest: String,
    args: String,
    notes: String,
    /// kind=local: file or folder to copy.
    local: String,
    /// kind=alias: id of the tool whose folder holds `exe`.
    of: String,
    mode: String,
    elevate: bool,
    /// "curl" to use curl's own User-Agent (SourceForge refuses browser UAs from scripts).
    ua: String,
    /// Extra files copied into the tool folder after extraction.
    extra: Vec<Extra>,
    /// Relative paths removed after extraction (updaters, installers, bundled junk).
    delete: Vec<String>,
    /// Staged on the drive but gets no launcher tile.
    hidden: bool,
}

#[derive(Deserialize, Clone)]
struct Extra {
    #[serde(default)]
    local: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    ua: String,
    /// Destination relative to the tool folder. Ends with '/' to mean "into this folder".
    to: String,
}

#[derive(Deserialize, Clone, Default)]
#[serde(default)]
struct BootEntry {
    name: String,
    url: String,
    local: String,
    file: String,
    /// File to pull out of the downloaded archive (by name; first match wins).
    pick: String,
    /// Destination relative to payload/. Default Boot/<name>.
    dest: String,
    version: String,
    source: String,
    notes: String,
    ua: String,
    /// Ventoy menu alias written later by CorvusSetup (kept here so one file lists everything).
    menu: String,
}

// ---- tools.toml output -----------------------------------------------------------------------

#[derive(Serialize)]
struct ToolOut {
    id: String,
    name: String,
    category: String,
    path: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    args: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    mode: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    version: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    source: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    sha256: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    license: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    elevate: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    notes: String,
}

#[derive(Serialize)]
struct ToolsFile {
    tool: Vec<ToolOut>,
}

// ---- main ------------------------------------------------------------------------------------

struct Opts {
    catalogue: PathBuf,
    only: Option<BTreeSet<String>>,
    tools: bool,
    boot: bool,
    download: bool,
    /// Keep tool folders that are already staged (exe present) instead of re-extracting.
    reuse: bool,
}

fn parse_args() -> Opts {
    let mut o = Opts {
        catalogue: workspace_root().join("build-tools/payload-catalogue.toml"),
        only: None,
        tools: true,
        boot: true,
        download: true,
        reuse: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--only" => {
                let v = it.next().unwrap_or_default();
                o.only = Some(v.split(',').map(|s| s.trim().to_string()).collect());
            }
            "--tools-only" => o.boot = false,
            "--boot-only" => o.tools = false,
            "--no-download" => o.download = false,
            "--reuse" => o.reuse = true,
            "--catalogue" => o.catalogue = PathBuf::from(it.next().unwrap_or_default()),
            other => {
                eprintln!("unknown argument {other}");
                std::process::exit(2);
            }
        }
    }
    o
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

struct Ctx {
    root: PathBuf,
    downloads: PathBuf,
    payload: PathBuf,
    sevenzip: PathBuf,
    download: bool,
    reuse: bool,
    report: Vec<String>,
}

fn main() {
    let opts = parse_args();
    let text = match fs::read_to_string(&opts.catalogue) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("cannot read {}: {e}", opts.catalogue.display());
            std::process::exit(1);
        }
    };
    let cat: Catalogue = match toml::from_str(&text) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("catalogue is not valid: {e}");
            std::process::exit(1);
        }
    };

    let root = workspace_root();
    let mut ctx = Ctx {
        downloads: root.join(&cat.settings.downloads),
        payload: root.join(&cat.settings.payload),
        sevenzip: PathBuf::from(&cat.settings.sevenzip),
        root,
        download: opts.download,
        reuse: opts.reuse,
        report: Vec::new(),
    };
    fs::create_dir_all(&ctx.downloads).ok();
    fs::create_dir_all(ctx.payload.join("Launcher").join("credits")).ok();
    fs::create_dir_all(ctx.payload.join("Boot")).ok();
    // Prefer the freshly staged 7-Zip (newest compression methods) over the bootstrap copy.
    let staged_7z = ctx
        .payload
        .join("Tools")
        .join("Disk")
        .join("7-Zip")
        .join("7z.exe");
    if staged_7z.is_file() {
        ctx.sevenzip = staged_7z;
    }
    if !ctx.sevenzip.is_file() {
        eprintln!("7z.exe not found at {}", ctx.sevenzip.display());
        std::process::exit(1);
    }
    println!("extractor: {}", ctx.sevenzip.display());

    let wanted = |id: &str| opts.only.as_ref().is_none_or(|s| s.contains(id));

    let mut out: Vec<ToolOut> = Vec::new();
    let mut staged_dests: std::collections::HashMap<String, PathBuf> = Default::default();
    let mut ok = 0usize;
    let mut failed: Vec<String> = Vec::new();

    if opts.tools {
        for e in &cat.tool {
            if !wanted(&e.id) {
                continue;
            }
            println!("\n=== {} ({}) ===", e.name, e.id);
            match stage_tool(&mut ctx, e, &staged_dests) {
                Ok(Some((dest, entry))) => {
                    staged_dests.insert(e.id.clone(), dest);
                    if !e.hidden {
                        out.push(entry);
                    }
                    ok += 1;
                }
                Ok(None) => ok += 1,
                Err(err) => {
                    println!("  FAILED: {err}");
                    ctx.report
                        .push(format!("| {} | FAILED | {} |", e.name, err));
                    failed.push(format!("{} — {err}", e.id));
                }
            }
        }
    }

    if opts.boot {
        for b in &cat.boot {
            if !wanted(&b.name) {
                continue;
            }
            println!("\n=== boot: {} ===", b.name);
            match stage_boot(&mut ctx, b) {
                Ok(()) => ok += 1,
                Err(err) => {
                    println!("  FAILED: {err}");
                    ctx.report
                        .push(format!("| {} | FAILED | {} |", b.name, err));
                    failed.push(format!("boot {} — {err}", b.name));
                }
            }
        }
    }

    // tools.toml (only when staging the full tool set, so --only never truncates the catalogue)
    if opts.tools && opts.only.is_none() {
        let path = ctx.payload.join("Launcher").join("tools.toml");
        let mut text = String::from(
            "# Corvus tool catalogue — generated by build-tools/stage-payload. Edit payload-catalogue.toml, not this.\n\n",
        );
        text.push_str(&toml::to_string_pretty(&ToolsFile { tool: out }).unwrap_or_default());
        if let Err(e) = fs::write(&path, text) {
            println!("could not write {}: {e}", path.display());
        } else {
            println!("\nwrote {}", path.display());
        }
        write_credits(&ctx, &cat);
    }

    // Launcher exe, if built
    let exe = ctx.root.join("target/release/Corvus.exe");
    if exe.is_file() {
        let to = ctx.payload.join("Launcher").join("Corvus.exe");
        match fs::copy(&exe, &to) {
            Ok(_) => println!("copied Corvus.exe into payload/Launcher"),
            Err(e) => println!("could not copy Corvus.exe: {e}"),
        }
    }

    // report
    let mut rep = String::from("# Staging report\n\n| Item | Status | Detail |\n|---|---|---|\n");
    for line in &ctx.report {
        rep.push_str(line);
        rep.push('\n');
    }
    let _ = fs::write(ctx.payload.join("STAGING-REPORT.md"), rep);

    println!("\n{ok} item(s) staged, {} failed", failed.len());
    for f in &failed {
        println!("  ✗ {f}");
    }
    if !failed.is_empty() {
        std::process::exit(1);
    }
}

// ---- tools -----------------------------------------------------------------------------------

fn stage_tool(
    ctx: &mut Ctx,
    e: &Entry,
    staged: &std::collections::HashMap<String, PathBuf>,
) -> Result<Option<(PathBuf, ToolOut)>, String> {
    let kind = if e.kind.is_empty() {
        "archive"
    } else {
        e.kind.as_str()
    };

    if kind == "alias" {
        let base = staged
            .get(&e.of)
            .cloned()
            .or_else(|| {
                // allow aliasing a tool staged in a previous run
                let d = default_dest(ctx, e, &e.of);
                d.is_dir().then_some(d)
            })
            .ok_or_else(|| format!("alias of '{}' but that tool is not staged", e.of))?;
        let exe = base.join(&e.exe);
        if !exe.is_file() {
            return Err(format!("alias target missing: {}", exe.display()));
        }
        let entry = tool_out(ctx, e, &base, &exe)?;
        ctx.report
            .push(format!("| {} | ok | alias → {} |", e.name, e.exe));
        return Ok(Some((base, entry)));
    }

    let dest = if e.dest.is_empty() {
        ctx.payload
            .join("Tools")
            .join(sanitize(&e.category))
            .join(sanitize(&e.name))
    } else {
        ctx.payload.join(e.dest.replace('/', "\\"))
    };

    // --reuse: an already-staged folder with the exe in place is taken as-is.
    if ctx.reuse && !e.exe.is_empty() {
        let exe = dest.join(e.exe.replace('/', "\\"));
        if exe.is_file() {
            let entry = tool_out(ctx, e, &dest, &exe)?;
            println!(
                "  reused   {} ({})",
                dest.strip_prefix(&ctx.payload).unwrap_or(&dest).display(),
                entry.version
            );
            ctx.report.push(format!(
                "| {} | ok (reused) | {} · {} |",
                e.name, entry.version, entry.path
            ));
            return Ok(Some((dest, entry)));
        }
    }

    // 1. obtain
    let staging = ctx.root.join("target").join("stage-tmp").join(&e.id);
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging).map_err(|x| x.to_string())?;

    match kind {
        "archive" | "exe" => {
            let file = fetch(ctx, &e.url, &e.post, &e.file, &e.ua)?;
            if kind == "exe" {
                let name = if e.exe.is_empty() {
                    file.file_name().unwrap().to_os_string()
                } else {
                    e.exe.clone().into()
                };
                fs::copy(&file, staging.join(name)).map_err(|x| x.to_string())?;
            } else {
                extract(ctx, &file, &staging)?;
            }
        }
        "local" => {
            let src = PathBuf::from(&e.local);
            if src.is_dir() {
                copy_dir(&src, &staging)?;
            } else if src.is_file() {
                let name = if e.exe.is_empty() {
                    src.file_name().unwrap().to_os_string()
                } else {
                    e.exe.clone().into()
                };
                fs::copy(&src, staging.join(name)).map_err(|x| x.to_string())?;
            } else {
                return Err(format!("local source not found: {}", src.display()));
            }
        }
        "manual" => {
            let src = ctx.downloads.join("manual").join(&e.id);
            if !src.is_dir()
                || fs::read_dir(&src)
                    .map(|mut d| d.next().is_none())
                    .unwrap_or(true)
            {
                return Err(format!(
                    "manual item: put the extracted program in {} ({})",
                    src.display(),
                    e.notes
                ));
            }
            copy_dir(&src, &staging)?;
        }
        other => return Err(format!("unknown kind '{other}'")),
    }

    // 2. shape: subdir, auto-strip, deletes, extras
    // Strip a lone wrapper folder first, then descend into the requested subdir, then strip again.
    let mut content = auto_strip(staging.clone());
    if !e.subdir.is_empty() {
        content = content.join(e.subdir.replace('/', "\\"));
        if !content.is_dir() {
            return Err(format!(
                "subdir '{}' not found in archive; top level has: {}",
                e.subdir,
                list_top(content.parent().unwrap_or(&staging))
            ));
        }
        content = auto_strip(content);
    }
    for d in &e.delete {
        let p = content.join(d.replace('/', "\\"));
        if p.is_dir() {
            let _ = fs::remove_dir_all(&p);
        } else {
            let _ = fs::remove_file(&p);
        }
    }
    for x in &e.extra {
        let to = content.join(x.to.trim_end_matches(['/', '\\']).replace('/', "\\"));
        let src = if !x.local.is_empty() {
            PathBuf::from(&x.local)
        } else {
            fetch(ctx, &x.url, "", "", &x.ua)?
        };
        if src.is_dir() {
            copy_dir(&src, &to)?;
        } else {
            let target = if x.to.ends_with('/') || x.to.ends_with('\\') {
                fs::create_dir_all(&to).ok();
                to.join(src.file_name().unwrap())
            } else {
                if let Some(p) = to.parent() {
                    fs::create_dir_all(p).ok();
                }
                to
            };
            fs::copy(&src, &target).map_err(|err| format!("extra {}: {err}", src.display()))?;
        }
    }

    // 3. verify exe
    let exe = content.join(e.exe.replace('/', "\\"));
    if !exe.is_file() {
        let listing = list_top(&content);
        return Err(format!(
            "exe '{}' not found after extraction; top level has: {listing}",
            e.exe
        ));
    }

    // 4. move into payload
    if dest.exists() {
        fs::remove_dir_all(&dest).map_err(|x| format!("clearing {}: {x}", dest.display()))?;
    }
    if let Some(p) = dest.parent() {
        fs::create_dir_all(p).ok();
    }
    if fs::rename(&content, &dest).is_err() {
        copy_dir(&content, &dest)?;
    }
    let _ = fs::remove_dir_all(&staging);

    let exe = dest.join(e.exe.replace('/', "\\"));
    let entry = tool_out(ctx, e, &dest, &exe)?;
    let size = dir_size(&dest);
    println!(
        "  staged → {}  ({:.1} MB, {})",
        dest.strip_prefix(&ctx.payload).unwrap_or(&dest).display(),
        size as f64 / 1e6,
        entry.version
    );
    ctx.report.push(format!(
        "| {} | ok | {} · {:.1} MB · {} |",
        e.name,
        entry.version,
        size as f64 / 1e6,
        entry.path
    ));
    Ok(Some((dest, entry)))
}

fn default_dest(ctx: &Ctx, _e: &Entry, of_id: &str) -> PathBuf {
    // Best effort: look for any Tools/*/<id-ish> folder. Only used for aliases across runs.
    let tools = ctx.payload.join("Tools");
    if let Ok(cats) = fs::read_dir(&tools) {
        for c in cats.flatten() {
            if let Ok(names) = fs::read_dir(c.path()) {
                for n in names.flatten() {
                    if n.file_name()
                        .to_string_lossy()
                        .to_ascii_lowercase()
                        .replace(' ', "")
                        == of_id.to_ascii_lowercase()
                    {
                        return n.path();
                    }
                }
            }
        }
    }
    tools.join("missing").join(of_id)
}

fn tool_out(ctx: &Ctx, e: &Entry, dest: &Path, exe: &Path) -> Result<ToolOut, String> {
    let rel = exe
        .strip_prefix(&ctx.payload)
        .map_err(|_| "exe outside payload".to_string())?
        .to_string_lossy()
        .replace('\\', "/");
    let sha = sha256_file(exe).map_err(|x| format!("hashing {}: {x}", exe.display()))?;
    let version = if e.version.is_empty() {
        exe_version(exe).unwrap_or_default()
    } else {
        e.version.clone()
    };
    let _ = dest;
    Ok(ToolOut {
        id: e.id.clone(),
        name: e.name.clone(),
        category: e.category.clone(),
        path: rel,
        args: e.args.clone(),
        mode: e.mode.clone(),
        version,
        source: e.source.clone(),
        sha256: sha,
        license: e.license.clone(),
        elevate: e.elevate,
        notes: e.notes.clone(),
    })
}

// ---- boot ------------------------------------------------------------------------------------

fn stage_boot(ctx: &mut Ctx, b: &BootEntry) -> Result<(), String> {
    let dest = if b.dest.is_empty() {
        ctx.payload.join("Boot").join(&b.name)
    } else {
        ctx.payload.join(b.dest.replace('/', "\\"))
    };
    let src = if !b.local.is_empty() {
        let p = PathBuf::from(&b.local);
        if !p.is_file() {
            return Err(format!("local file not found: {}", p.display()));
        }
        p
    } else {
        fetch(ctx, &b.url, "", &b.file, &b.ua)?
    };

    if let Some(p) = dest.parent() {
        fs::create_dir_all(p).ok();
    }
    if b.pick.is_empty() {
        copy_if_changed(&src, &dest)?;
    } else {
        let tmp = ctx
            .root
            .join("target")
            .join("stage-tmp")
            .join(format!("boot-{}", b.name));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).map_err(|x| x.to_string())?;
        extract(ctx, &src, &tmp)?;
        let found = find_file(&tmp, &b.pick)
            .ok_or_else(|| format!("'{}' not found inside {}", b.pick, src.display()))?;
        copy_if_changed(&found, &dest)?;
        let _ = fs::remove_dir_all(&tmp);
    }
    let size = fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
    let sha = sha256_file(&dest).unwrap_or_default();
    println!("  staged → Boot/{}  ({:.1} MB)", b.name, size as f64 / 1e6);
    ctx.report.push(format!(
        "| Boot/{} | ok | {} · {:.1} MB · sha256 {} |",
        b.name,
        b.version,
        size as f64 / 1e6,
        &sha[..16]
    ));
    Ok(())
}

// ---- helpers ---------------------------------------------------------------------------------

/// Download (or reuse from cache). Returns the cached file path.
fn fetch(ctx: &Ctx, url: &str, post: &str, file: &str, ua: &str) -> Result<PathBuf, String> {
    if url.is_empty() {
        return Err("no url".into());
    }
    let name = if file.is_empty() {
        url.rsplit('/')
            .next()
            .unwrap_or("download")
            .split('?')
            .next()
            .unwrap_or("download")
            .to_string()
    } else {
        file.to_string()
    };
    let target = ctx.downloads.join(&name);
    if target.is_file() && fs::metadata(&target).map(|m| m.len() > 0).unwrap_or(false) {
        println!("  cached   {name}");
        return Ok(target);
    }
    if !ctx.download {
        return Err(format!("{name} not in cache and --no-download given"));
    }
    let agent = match ua {
        "curl" => None,
        "" => Some(BROWSER_UA.to_string()),
        other => Some(other.to_string()),
    };

    let mut real_url = url.to_string();
    if !post.is_empty() {
        // POST without following; grab Location.
        let mut c = curl_cmd(&agent);
        c.args(["-s", "-o", "NUL", "-D", "-", "-X", "POST", "-d", post, url]);
        let outp = c.output().map_err(|e| format!("curl: {e}"))?;
        let headers = String::from_utf8_lossy(&outp.stdout);
        real_url = headers
            .lines()
            .find_map(|l| {
                let (k, v) = l.split_once(':')?;
                k.eq_ignore_ascii_case("location")
                    .then(|| v.trim().to_string())
            })
            .ok_or_else(|| format!("POST to {url} gave no Location header"))?;
        println!("  redirect {real_url}");
    }

    println!("  download {name}");
    let part = ctx.downloads.join(format!("{name}.part"));
    let _ = fs::remove_file(&part);
    let mut c = curl_cmd(&agent);
    c.args([
        "-L",
        "--fail",
        "--retry",
        "3",
        "--retry-delay",
        "3",
        "--connect-timeout",
        "30",
        "-#",
        "-o",
    ]);
    c.arg(&part).arg(&real_url);
    let status = c.status().map_err(|e| format!("curl: {e}"))?;
    if !status.success() {
        let _ = fs::remove_file(&part);
        return Err(format!("download failed ({status}) for {real_url}"));
    }
    let len = fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
    if len < 1024 {
        let peek = fs::read(&part).unwrap_or_default();
        let _ = fs::remove_file(&part);
        return Err(format!(
            "download too small ({len} bytes): {}",
            String::from_utf8_lossy(&peek[..peek.len().min(120)]).replace('\n', " ")
        ));
    }
    fs::rename(&part, &target).map_err(|e| e.to_string())?;
    Ok(target)
}

fn curl_cmd(agent: &Option<String>) -> Command {
    let mut c = Command::new("curl.exe");
    if let Some(a) = agent {
        c.arg("-A").arg(a);
    }
    c
}

fn extract(ctx: &Ctx, archive: &Path, into: &Path) -> Result<(), String> {
    let out = Command::new(&ctx.sevenzip)
        .arg("x")
        .arg("-y")
        .arg("-bso0")
        .arg("-bsp0")
        .arg(format!("-o{}", into.display()))
        .arg(archive)
        .output()
        .map_err(|e| format!("7z: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "7z failed on {}: {}",
            archive.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(())
}

/// If a folder holds exactly one sub-folder and nothing else, descend into it (repeat).
fn auto_strip(mut dir: PathBuf) -> PathBuf {
    loop {
        let Ok(rd) = fs::read_dir(&dir) else {
            return dir;
        };
        let entries: Vec<_> = rd.flatten().collect();
        if entries.len() == 1 && entries[0].path().is_dir() {
            dir = entries[0].path();
        } else {
            return dir;
        }
    }
}

fn list_top(dir: &Path) -> String {
    let mut v: Vec<String> = fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| {
                    let n = e.file_name().to_string_lossy().to_string();
                    if e.path().is_dir() {
                        format!("{n}/")
                    } else {
                        n
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v.truncate(12);
    v.join(", ")
}

fn find_file(dir: &Path, name: &str) -> Option<PathBuf> {
    for e in fs::read_dir(dir).ok()?.flatten() {
        let p = e.path();
        if p.is_dir() {
            if let Some(f) = find_file(&p, name) {
                return Some(f);
            }
        } else if p
            .file_name()
            .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(name))
        {
            return Some(p);
        }
    }
    None
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    fs::create_dir_all(to).map_err(|e| format!("mkdir {}: {e}", to.display()))?;
    for e in fs::read_dir(from)
        .map_err(|e| format!("{}: {e}", from.display()))?
        .flatten()
    {
        let src = e.path();
        let dst = to.join(e.file_name());
        if src.is_dir() {
            copy_dir(&src, &dst)?;
        } else {
            fs::copy(&src, &dst).map_err(|err| format!("copy {}: {err}", src.display()))?;
        }
    }
    Ok(())
}

/// Copy unless the destination already has the same size (large ISOs re-run quickly).
fn copy_if_changed(src: &Path, dst: &Path) -> Result<(), String> {
    let s = fs::metadata(src).map(|m| m.len()).unwrap_or(0);
    let d = fs::metadata(dst).map(|m| m.len()).unwrap_or(u64::MAX);
    if s == d {
        println!(
            "  unchanged {}",
            dst.file_name().unwrap_or_default().to_string_lossy()
        );
        return Ok(());
    }
    fs::copy(src, dst)
        .map(|_| ())
        .map_err(|e| format!("copy {} → {}: {e}", src.display(), dst.display()))
}

fn dir_size(dir: &Path) -> u64 {
    fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| {
                    let p = e.path();
                    if p.is_dir() {
                        dir_size(&p)
                    } else {
                        fs::metadata(&p).map(|m| m.len()).unwrap_or(0)
                    }
                })
                .sum()
        })
        .unwrap_or(0)
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '-',
            c => c,
        })
        .collect::<String>()
        .trim()
        .to_string()
}

fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut f = fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// ProductVersion from the exe's version resource, via PowerShell (build-time only).
fn exe_version(exe: &Path) -> Option<String> {
    let script = format!(
        "$v=(Get-Item -LiteralPath '{}').VersionInfo; if($v.ProductVersion){{$v.ProductVersion}}else{{$v.FileVersion}}",
        exe.display().to_string().replace('\'', "''")
    );
    let out = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let s = s.split('+').next().unwrap_or(&s).trim().to_string();
    (!s.is_empty()).then_some(s)
}

/// credits/LICENSES.md plus any licence text files found at the top of each tool folder.
fn write_credits(ctx: &Ctx, cat: &Catalogue) {
    let credits = ctx.payload.join("Launcher").join("credits");
    let mut md = String::from("# Corvus credits\n\nEvery program on this drive, where it came from and under what terms.\n\n| Tool | Version | Licence | Source |\n|---|---|---|---|\n");
    for e in &cat.tool {
        if e.hidden {
            continue;
        }
        md.push_str(&format!(
            "| {} | {} | {} | {} |\n",
            e.name, e.version, e.license, e.source
        ));
        // copy licence files
        let dest = if e.dest.is_empty() {
            ctx.payload
                .join("Tools")
                .join(sanitize(&e.category))
                .join(sanitize(&e.name))
        } else {
            ctx.payload.join(e.dest.replace('/', "\\"))
        };
        if let Ok(rd) = fs::read_dir(&dest) {
            for f in rd.flatten() {
                let n = f.file_name().to_string_lossy().to_ascii_lowercase();
                let is_lic = f.path().is_file()
                    && (n.starts_with("license")
                        || n.starts_with("licence")
                        || n.starts_with("copying")
                        || n.starts_with("eula")
                        || n.starts_with("gpl")
                        || n.starts_with("lgpl"));
                if is_lic {
                    let to = credits.join(&e.id);
                    fs::create_dir_all(&to).ok();
                    let _ = fs::copy(f.path(), to.join(f.file_name()));
                }
            }
        }
    }
    md.push_str("\n## Boot images\n\n| Image | Version | Source |\n|---|---|---|\n");
    for b in &cat.boot {
        md.push_str(&format!("| {} | {} | {} |\n", b.name, b.version, b.source));
    }
    md.push_str("\n## Foundation\n\n- Ventoy — GPL-3.0 — https://www.ventoy.net\n- egui / eframe — MIT OR Apache-2.0 — https://github.com/emilk/egui\n- Corvus — MIT — Aiden\n\nThanks to MediCat USB, which carried this bench for years.\n");
    let _ = fs::write(credits.join("LICENSES.md"), md);
    let _ = std::io::stdout().flush();
}
