//! Agent distributions — package an agent as a shareable artifact and install it
//! (initiative #13, phase 4). Modeled on Hermes "profile distributions".
//!
//! The decisive property is **safety**: a distribution carries only the
//! *shareable* parts of an agent (its identity and behavior) and **never** ships
//! private runtime state — memory, sessions, credentials, or the active pointer.
//! Importing materializes a NEW agent that starts with empty memory/sessions and
//! no creds (created lazily on first use, exactly like a freshly created agent).
//!
//! ## Artifact format
//!
//! A distribution is a single, self-contained **`.tar` archive** (USTAR, the
//! POSIX tar format), written and read by the minimal std-only implementation in
//! this module. We deliberately avoid adding a new archive dependency: the
//! workspace ships neither `tar` nor `flate2`, and a heavy dep for this one
//! feature is not warranted. A plain `.tar` is a single shareable file (better
//! than a loose directory tree), is trivially inspectable with standard tools,
//! and our writer emits entries in a **deterministic, sorted order** so two
//! exports of the same agent are byte-identical. Compression is intentionally
//! omitted (no `.gz`) to keep the implementation dependency-free; a `.tar`
//! compresses well after the fact if a user wants to `gzip` it.
//!
//! ## Included vs hard-excluded
//!
//! INCLUDED (the shareable agent):
//! - `agent.toml` — the manifest (name/description/created_at + a distribution
//!   `version`), rewritten on export so the artifact is portable.
//! - `config.toml` — layered behavior config.
//! - `persona.md` — the durable identity.
//! - `skills/` — learned procedural memory (whole subtree, if present).
//!
//! HARD-EXCLUDED (never shipped — the safety contract):
//! - `memory.sqlite` (and `-wal` / `-shm` siblings) — the growing memory DB.
//! - any sessions store / `sessions/` dir — past conversations.
//! - `.env` — credentials.
//! - `active_agent` — the sticky pointer (a base-home concern anyway).
//! - logs / caches — anything not on the include list.
//!
//! Export is **allow-list based**: only the four included paths are ever walked,
//! so a new private artifact added to an agent home in the future is excluded by
//! default rather than accidentally shipped.

use std::{
    fs,
    path::{Path, PathBuf},
};

use super::registry::{self, AgentMeta, RegistryResult, validate_name};

/// The distribution manifest format version embedded in the exported
/// `agent.toml`. Bumped if the artifact layout changes incompatibly.
pub const DISTRIBUTION_VERSION: u32 = 1;

const AGENT_META_FILE: &str = "agent.toml";
const CONFIG_FILE: &str = "config.toml";
const PERSONA_FILE: &str = "persona.md";
const SKILLS_DIR: &str = "skills";

/// The default artifact extension. A single `.tar` file (see module docs).
pub const ARTIFACT_EXT: &str = "tar";

/// The exact set of top-level entries that may appear in a distribution. Export
/// only ever reads these; import only ever writes these. Anything else in a
/// source home is excluded, and anything else found in an artifact on import is
/// ignored (defense in depth).
const INCLUDED_FILES: &[&str] = &[AGENT_META_FILE, CONFIG_FILE, PERSONA_FILE];

/// Paths that must NEVER appear in an artifact. Not used to *filter* (export is
/// allow-list based), but asserted by tests and rejected on import as a final
/// guard against a hand-crafted malicious artifact.
pub const EXCLUDED_PREFIXES: &[&str] = &[
    "memory.sqlite",
    "memory.sqlite-wal",
    "memory.sqlite-shm",
    "sessions",
    ".env",
    "active_agent",
];

/// Export the shareable parts of agent `name` (under `base`) into a `.tar`
/// artifact at `output`. Returns the path written.
///
/// The artifact contains exactly `agent.toml` (rewritten with a distribution
/// `version`), `config.toml`, `persona.md`, and the `skills/` subtree if present
/// — and nothing else. Private runtime state is never read.
pub fn export_agent(base: &Path, name: &str, output: &Path) -> RegistryResult<PathBuf> {
    validate_name(name)?;
    if !registry::agent_exists(base, name) {
        return Err(format!("agent not found: `{name}`"));
    }
    let home = registry::agent_home(base, name);

    let mut builder = TarBuilder::new();

    // Manifest: re-read the agent's metadata and stamp the distribution version,
    // so the artifact is a portable, self-describing manifest.
    let meta = read_meta(&home, name)?;
    let manifest = DistributionManifest::from_meta(&meta);
    let manifest_toml = toml::to_string_pretty(&manifest)
        .map_err(|e| format!("failed to serialize manifest: {e}"))?;
    builder.add_file(AGENT_META_FILE, manifest_toml.as_bytes());

    // config.toml + persona.md (skip silently if a home is missing one).
    for file in [CONFIG_FILE, PERSONA_FILE] {
        let path = home.join(file);
        if path.is_file() {
            let bytes =
                fs::read(&path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
            builder.add_file(file, &bytes);
        }
    }

    // skills/ subtree (procedural memory), deterministically ordered.
    let skills = home.join(SKILLS_DIR);
    if skills.is_dir() {
        add_dir_recursive(&mut builder, &skills, SKILLS_DIR)?;
    }

    let bytes = builder.finish();
    if let Some(parent) = output.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
    }
    fs::write(output, &bytes).map_err(|e| format!("failed to write {}: {e}", output.display()))?;
    Ok(output.to_path_buf())
}

/// Import an exported artifact at `path` into a NEW agent under `base`.
///
/// The new agent's name is `name_override` if given, else the manifest's name;
/// it is validated and must not already exist. Only `config.toml`, `persona.md`,
/// and `skills/` are materialized — memory, sessions, and `.env` are NEVER
/// created or copied, so the new agent starts with empty private state.
pub fn import_agent(
    base: &Path,
    path: &Path,
    name_override: Option<&str>,
) -> RegistryResult<registry::AgentInfo> {
    let bytes =
        fs::read(path).map_err(|e| format!("failed to read artifact {}: {e}", path.display()))?;
    let entries = TarReader::parse(&bytes)?;

    // Locate + parse the manifest to derive the default name.
    let manifest_bytes = entries
        .iter()
        .find(|(name, _)| name == AGENT_META_FILE)
        .map(|(_, data)| data.clone())
        .ok_or_else(|| format!("artifact has no {AGENT_META_FILE} manifest"))?;
    let manifest: DistributionManifest = toml::from_str(
        std::str::from_utf8(&manifest_bytes)
            .map_err(|e| format!("manifest is not valid UTF-8: {e}"))?,
    )
    .map_err(|e| format!("failed to parse manifest: {e}"))?;

    let name = match name_override {
        Some(n) => n.to_string(),
        None => manifest.name.clone(),
    };
    validate_name(&name)?;
    if registry::agent_exists(base, &name) {
        return Err(format!(
            "agent already exists: `{name}` (choose a different --name)"
        ));
    }

    let home = registry::agent_home(base, &name);
    fs::create_dir_all(&home).map_err(|e| format!("failed to create {}: {e}", home.display()))?;

    // Materialize ONLY allow-listed entries. A malicious artifact carrying
    // excluded private paths is rejected outright (the safety contract is total,
    // not best-effort).
    for (entry_name, data) in &entries {
        if is_excluded(entry_name) {
            // Clean up the half-created home so a rejected import leaves no trace.
            let _ = fs::remove_dir_all(&home);
            return Err(format!(
                "refusing to import: artifact contains excluded private path `{entry_name}`"
            ));
        }
        let is_skill =
            entry_name == SKILLS_DIR || entry_name.starts_with(&format!("{SKILLS_DIR}/"));
        let allowed = INCLUDED_FILES.contains(&entry_name.as_str()) || is_skill;
        if !allowed {
            continue; // ignore anything outside the allow-list.
        }
        if entry_name == AGENT_META_FILE {
            continue; // written from the manifest below with the new name.
        }
        write_entry(&home, entry_name, data)?;
    }

    // Write a fresh agent.toml carrying the (possibly overridden) name and the
    // imported description/created_at preserved from the manifest.
    let meta = AgentMeta {
        name: name.clone(),
        description: manifest.description.clone(),
        created_at: manifest.created_at,
    };
    let text =
        toml::to_string_pretty(&meta).map_err(|e| format!("failed to serialize meta: {e}"))?;
    fs::write(home.join(AGENT_META_FILE), text)
        .map_err(|e| format!("failed to write {AGENT_META_FILE}: {e}"))?;

    registry::agent_info(base, &name)
        .ok_or_else(|| "imported agent could not be read back".to_string())
}

/// The default output path for `export <name>`: `./<name>-agent.tar`.
pub fn default_output_path(name: &str) -> PathBuf {
    PathBuf::from(format!("{name}-agent.{ARTIFACT_EXT}"))
}

/// Whether `entry_name` matches a hard-excluded private path.
fn is_excluded(entry_name: &str) -> bool {
    EXCLUDED_PREFIXES
        .iter()
        .any(|prefix| entry_name == *prefix || entry_name.starts_with(&format!("{prefix}/")))
}

/// The on-disk manifest written into the artifact's `agent.toml`. Superset of
/// [`AgentMeta`] with a distribution `version`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct DistributionManifest {
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    created_at: u64,
    /// Distribution format version (see [`DISTRIBUTION_VERSION`]).
    version: u32,
}

impl DistributionManifest {
    fn from_meta(meta: &AgentMeta) -> Self {
        Self {
            name: meta.name.clone(),
            description: meta.description.clone(),
            created_at: meta.created_at,
            version: DISTRIBUTION_VERSION,
        }
    }
}

fn read_meta(home: &Path, name: &str) -> RegistryResult<AgentMeta> {
    let text = fs::read_to_string(home.join(AGENT_META_FILE))
        .map_err(|e| format!("failed to read manifest for `{name}`: {e}"))?;
    toml::from_str(&text).map_err(|e| format!("failed to parse manifest for `{name}`: {e}"))
}

/// Write one artifact entry into the agent home at the (already allow-listed)
/// relative `entry_name`, creating parent dirs as needed. Defensively re-checks
/// the path stays within `home`.
fn write_entry(home: &Path, entry_name: &str, data: &[u8]) -> RegistryResult<()> {
    // Reject traversal / absolute paths in artifact entry names.
    if entry_name.contains("..") || entry_name.starts_with('/') || entry_name.contains('\\') {
        return Err(format!("unsafe entry path in artifact: `{entry_name}`"));
    }
    let dest = home.join(entry_name);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
    }
    fs::write(&dest, data).map_err(|e| format!("failed to write {}: {e}", dest.display()))
}

/// Recursively add a directory subtree to the tar builder under `rel_root`,
/// entries sorted for determinism.
fn add_dir_recursive(builder: &mut TarBuilder, dir: &Path, rel_root: &str) -> RegistryResult<()> {
    let mut entries: Vec<(String, PathBuf, bool)> = Vec::new();
    let read = fs::read_dir(dir).map_err(|e| format!("failed to read {}: {e}", dir.display()))?;
    for entry in read.flatten() {
        let path = entry.path();
        let Some(file_name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        let is_dir = path.is_dir();
        let rel = format!("{rel_root}/{file_name}");
        entries.push((rel, path, is_dir));
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    for (rel, path, is_dir) in entries {
        if is_dir {
            add_dir_recursive(builder, &path, &rel)?;
        } else {
            let bytes =
                fs::read(&path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
            builder.add_file(&rel, &bytes);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Minimal std-only USTAR tar writer / reader.
//
// We support exactly what distributions need: regular files (type '0') with a
// relative path, mode 0644, and zeroed owner/time fields for deterministic,
// reproducible output. No symlinks, no long-name (GNU) extensions — entry names
// here are short, controlled relative paths well under the 100-byte USTAR limit.
// ---------------------------------------------------------------------------

const BLOCK: usize = 512;

struct TarBuilder {
    out: Vec<u8>,
}

impl TarBuilder {
    fn new() -> Self {
        Self { out: Vec::new() }
    }

    /// Append a regular-file entry. `name` is a relative path (<100 bytes).
    fn add_file(&mut self, name: &str, data: &[u8]) {
        let mut header = [0u8; BLOCK];
        let name_bytes = name.as_bytes();
        // name field: bytes 0..100.
        let n = name_bytes.len().min(100);
        header[0..n].copy_from_slice(&name_bytes[..n]);
        // mode (8): octal "0000644\0".
        write_octal(&mut header[100..108], 0o644);
        // uid (8), gid (8): zero.
        write_octal(&mut header[108..116], 0);
        write_octal(&mut header[116..124], 0);
        // size (12): octal.
        write_octal(&mut header[124..136], data.len() as u64);
        // mtime (12): zero for determinism.
        write_octal(&mut header[136..148], 0);
        // typeflag (1): '0' regular file.
        header[156] = b'0';
        // USTAR magic + version.
        header[257..263].copy_from_slice(b"ustar\0");
        header[263..265].copy_from_slice(b"00");

        // checksum: spaces while computing, then octal in 148..156 (6 octal
        // digits, NUL, space).
        for b in header[148..156].iter_mut() {
            *b = b' ';
        }
        let sum: u32 = header.iter().map(|&b| b as u32).sum();
        let chk = format!("{sum:06o}\0 ");
        header[148..156].copy_from_slice(chk.as_bytes());

        self.out.extend_from_slice(&header);
        self.out.extend_from_slice(data);
        // pad data to a 512 boundary.
        let rem = data.len() % BLOCK;
        if rem != 0 {
            self.out.extend(std::iter::repeat_n(0u8, BLOCK - rem));
        }
    }

    /// Finish the archive: two zero blocks terminate a tar stream.
    fn finish(mut self) -> Vec<u8> {
        self.out.extend(std::iter::repeat_n(0u8, BLOCK * 2));
        self.out
    }
}

fn write_octal(field: &mut [u8], value: u64) {
    // USTAR numeric fields: octal ASCII, NUL-terminated, right-justified,
    // zero-padded. Width is field.len()-1 digits + trailing NUL.
    let width = field.len() - 1;
    let s = format!("{value:0width$o}", width = width);
    let bytes = s.as_bytes();
    // Take the last `width` bytes in case of overflow (won't happen here).
    let start = bytes.len().saturating_sub(width);
    field[..width].copy_from_slice(&bytes[start..]);
    field[width] = 0;
}

struct TarReader;

impl TarReader {
    /// Parse a tar archive into `(name, data)` pairs for regular-file entries.
    fn parse(bytes: &[u8]) -> RegistryResult<Vec<(String, Vec<u8>)>> {
        let mut entries = Vec::new();
        let mut offset = 0usize;
        while offset + BLOCK <= bytes.len() {
            let header = &bytes[offset..offset + BLOCK];
            // Two consecutive zero blocks terminate the archive.
            if header.iter().all(|&b| b == 0) {
                break;
            }
            offset += BLOCK;

            let name = read_str(&header[0..100]);
            let size = read_octal(&header[124..136])?;
            let typeflag = header[156];

            let data_start = offset;
            let data_end = data_start + size as usize;
            if data_end > bytes.len() {
                return Err("truncated tar entry".to_string());
            }
            // type '0' or NUL (legacy) is a regular file; dirs ('5') carry no
            // data here, and we synthesize dirs from file paths on extract.
            if typeflag == b'0' || typeflag == 0 {
                if name.is_empty() {
                    return Err("tar entry with empty name".to_string());
                }
                entries.push((name, bytes[data_start..data_end].to_vec()));
            }
            // advance past data, padded to a block boundary.
            let mut advance = size as usize;
            let rem = advance % BLOCK;
            if rem != 0 {
                advance += BLOCK - rem;
            }
            offset += advance;
        }
        Ok(entries)
    }
}

fn read_str(field: &[u8]) -> String {
    let end = field.iter().position(|&b| b == 0).unwrap_or(field.len());
    String::from_utf8_lossy(&field[..end]).into_owned()
}

fn read_octal(field: &[u8]) -> RegistryResult<u64> {
    let s = read_str(field);
    let trimmed = s.trim_matches(|c: char| c == ' ' || c == '\0');
    if trimmed.is_empty() {
        return Ok(0);
    }
    u64::from_str_radix(trimmed, 8).map_err(|e| format!("invalid octal tar field `{trimmed}`: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::registry::{CreateOptions, create_agent};

    fn base() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    /// Seed an agent home with private runtime state to prove export excludes it.
    fn seed_private(home: &Path) {
        fs::write(home.join("memory.sqlite"), b"SQLite format 3\0secrets").unwrap();
        fs::write(home.join("memory.sqlite-wal"), b"wal").unwrap();
        fs::write(home.join("memory.sqlite-shm"), b"shm").unwrap();
        fs::write(home.join(".env"), b"OPENAI_API_KEY=sk-secret").unwrap();
        fs::create_dir_all(home.join("sessions")).unwrap();
        fs::write(home.join("sessions").join("s1.json"), b"{\"private\":true}").unwrap();
        fs::write(home.join("active_agent"), b"someone").unwrap();
    }

    #[test]
    fn export_includes_shareable_and_excludes_private() {
        let dir = base();
        let base = dir.path();
        let info = create_agent(
            base,
            "alice",
            &CreateOptions {
                persona: Some("# Persona: alice\nbe helpful\n".to_string()),
                provider: Some("openrouter".to_string()),
                ..Default::default()
            },
        )
        .expect("create");
        // a skill (procedural memory) — should be included.
        fs::create_dir_all(info.home.join("skills").join("greet")).unwrap();
        fs::write(
            info.home.join("skills").join("greet").join("SKILL.md"),
            "say hi",
        )
        .unwrap();
        // private state — must NOT be shipped.
        seed_private(&info.home);

        let out = base.join("alice-agent.tar");
        export_agent(base, "alice", &out).expect("export");

        let bytes = fs::read(&out).unwrap();
        let entries = TarReader::parse(&bytes).expect("parse");
        let names: Vec<&str> = entries.iter().map(|(n, _)| n.as_str()).collect();

        // Included.
        assert!(names.contains(&"agent.toml"), "names: {names:?}");
        assert!(names.contains(&"config.toml"), "names: {names:?}");
        assert!(names.contains(&"persona.md"), "names: {names:?}");
        assert!(names.contains(&"skills/greet/SKILL.md"), "names: {names:?}");

        // Hard-excluded: assert NONE of the private paths appear anywhere.
        for (name, data) in &entries {
            assert!(
                !is_excluded(name),
                "excluded path leaked into artifact: {name}"
            );
            // belt-and-suspenders: the secret bytes must not appear at all.
            assert!(
                !data.windows(9).any(|w| w == b"sk-secret"),
                "credential bytes leaked into entry {name}"
            );
        }
        // explicit names.
        for bad in ["memory.sqlite", ".env", "sessions/s1.json", "active_agent"] {
            assert!(!names.contains(&bad), "private path shipped: {bad}");
        }

        // manifest carries a distribution version.
        let manifest = entries.iter().find(|(n, _)| n == "agent.toml").unwrap();
        let parsed: DistributionManifest =
            toml::from_str(std::str::from_utf8(&manifest.1).unwrap()).unwrap();
        assert_eq!(parsed.version, DISTRIBUTION_VERSION);
        assert_eq!(parsed.name, "alice");
    }

    #[test]
    fn import_creates_agent_with_empty_private_state() {
        let dir = base();
        let base = dir.path();
        let info = create_agent(
            base,
            "src",
            &CreateOptions {
                persona: Some("# Persona: src\nvoice\n".to_string()),
                ..Default::default()
            },
        )
        .expect("create");
        seed_private(&info.home);
        let out = base.join("src-agent.tar");
        export_agent(base, "src", &out).expect("export");

        let imported = import_agent(base, &out, Some("dst")).expect("import");
        assert_eq!(imported.name, "dst");
        // persona/config materialized.
        assert!(imported.home.join("persona.md").is_file());
        assert!(imported.home.join("config.toml").is_file());
        // EMPTY private state: no memory, no sessions, no creds copied.
        assert!(!imported.home.join("memory.sqlite").exists());
        assert!(!imported.home.join("memory.sqlite-wal").exists());
        assert!(!imported.home.join(".env").exists());
        assert!(!imported.home.join("sessions").exists());
        assert!(!imported.home.join("active_agent").exists());
    }

    #[test]
    fn round_trip_preserves_persona_and_skills_drops_memory() {
        let dir = base();
        let base = dir.path();
        let persona = "# Persona: A\nI am agent A with a specific voice.\n";
        let info = create_agent(
            base,
            "a",
            &CreateOptions {
                persona: Some(persona.to_string()),
                description: Some("specialist".to_string()),
                ..Default::default()
            },
        )
        .expect("create");
        fs::create_dir_all(info.home.join("skills").join("k")).unwrap();
        fs::write(info.home.join("skills").join("k").join("SKILL.md"), "proc").unwrap();
        seed_private(&info.home);

        let out = base.join("a-agent.tar");
        export_agent(base, "a", &out).expect("export");
        let b = import_agent(base, &out, Some("b")).expect("import");

        // B's persona == A's persona.
        assert_eq!(
            fs::read_to_string(b.home.join("persona.md")).unwrap(),
            persona
        );
        // B's skills == A's skills.
        assert_eq!(
            fs::read_to_string(b.home.join("skills").join("k").join("SKILL.md")).unwrap(),
            "proc"
        );
        // description preserved via manifest.
        assert_eq!(b.description.as_deref(), Some("specialist"));
        // B's memory absent.
        assert!(!b.home.join("memory.sqlite").exists());
        assert!(!b.home.join("sessions").exists());
    }

    #[test]
    fn import_refuses_existing_name() {
        let dir = base();
        let base = dir.path();
        create_agent(base, "a", &CreateOptions::default()).expect("create a");
        let out = base.join("a-agent.tar");
        export_agent(base, "a", &out).expect("export");
        // default name from manifest == "a", which already exists.
        let err = import_agent(base, &out, None).unwrap_err();
        assert!(err.contains("already exists"), "err: {err}");
    }

    #[test]
    fn import_validates_name_override() {
        let dir = base();
        let base = dir.path();
        create_agent(base, "a", &CreateOptions::default()).expect("create a");
        let out = base.join("a-agent.tar");
        export_agent(base, "a", &out).expect("export");
        let err = import_agent(base, &out, Some("../escape")).unwrap_err();
        assert!(!err.is_empty());
        assert!(!registry::agent_exists(base, "../escape"));
    }

    #[test]
    fn name_override_changes_imported_name() {
        let dir = base();
        let base = dir.path();
        create_agent(base, "orig", &CreateOptions::default()).expect("create");
        let out = base.join("orig-agent.tar");
        export_agent(base, "orig", &out).expect("export");
        let imported = import_agent(base, &out, Some("renamed")).expect("import");
        assert_eq!(imported.name, "renamed");
        assert_eq!(
            registry::agent_info(base, "renamed").unwrap().name,
            "renamed"
        );
    }

    #[test]
    fn import_rejects_artifact_with_excluded_path() {
        // A hand-crafted malicious artifact carrying a private path is refused,
        // and leaves no agent home behind.
        let dir = base();
        let base = dir.path();
        let mut builder = TarBuilder::new();
        let manifest = DistributionManifest {
            name: "evil".to_string(),
            description: None,
            created_at: 1,
            version: DISTRIBUTION_VERSION,
        };
        builder.add_file(
            "agent.toml",
            toml::to_string_pretty(&manifest).unwrap().as_bytes(),
        );
        builder.add_file(".env", b"OPENAI_API_KEY=sk-evil");
        let bytes = builder.finish();
        let out = base.join("evil.tar");
        fs::write(&out, &bytes).unwrap();

        let err = import_agent(base, &out, None).unwrap_err();
        assert!(err.contains("excluded private path"), "err: {err}");
        assert!(!registry::agent_exists(base, "evil"));
        assert!(!registry::agent_home(base, "evil").exists());
    }

    #[test]
    fn export_is_deterministic() {
        let dir = base();
        let base = dir.path();
        let info = create_agent(base, "d", &CreateOptions::default()).expect("create");
        fs::create_dir_all(info.home.join("skills").join("x")).unwrap();
        fs::write(info.home.join("skills").join("x").join("SKILL.md"), "x").unwrap();
        let o1 = base.join("d1.tar");
        let o2 = base.join("d2.tar");
        export_agent(base, "d", &o1).expect("export 1");
        export_agent(base, "d", &o2).expect("export 2");
        assert_eq!(fs::read(&o1).unwrap(), fs::read(&o2).unwrap());
    }
}
