//! Read-only binary provenance. Never execute an update to discover its identity.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{io::Read, path::Path};

pub const GPUI_REV: &str = env!("TEZBAR_NATIVE_GPUI_POLICY_REV");
pub const RUST_VERSION: &str = env!("TEZBAR_NATIVE_RUST_POLICY_VERSION");
const START: &[u8] = b"\0TEZBAR_NATIVE_BUILD_V2\0";
const END: &[u8] = b"\0TEZBAR_NATIVE_BUILD_END\0";
const RECORD_LIMIT: usize = 4096;

// Referenced by current() through black_box so link-time dead stripping cannot
// remove it. Feature values come from this compilation, never from a sidecar.
macro_rules! record {
    ($bench:literal, $shaders:literal) => {
        concat!(
            "\0TEZBAR_NATIVE_BUILD_V2\0{\"schema\":2,\"version\":\"",
            env!("CARGO_PKG_VERSION"),
            "\",\"sourceCommit\":\"",
            env!("TEZBAR_NATIVE_GIT_SHA"),
            "\",\"dirty\":",
            env!("TEZBAR_NATIVE_DIRTY"),
            ",\"benchControl\":",
            $bench,
            ",\"runtimeShaders\":",
            $shaders,
            ",\"gpuiRev\":\"",
            env!("TEZBAR_NATIVE_GPUI_REV"),
            "\",\"rustVersion\":\"",
            env!("TEZBAR_NATIVE_RUST_VERSION"),
            "\"}\0TEZBAR_NATIVE_BUILD_END\0"
        )
    };
}
#[cfg(all(feature = "bench-control", feature = "runtime-shaders"))]
#[used]
static EMBEDDED: &str = record!("true", "true");
#[cfg(all(feature = "bench-control", not(feature = "runtime-shaders")))]
#[used]
static EMBEDDED: &str = record!("true", "false");
#[cfg(all(not(feature = "bench-control"), feature = "runtime-shaders"))]
#[used]
static EMBEDDED: &str = record!("false", "true");
#[cfg(all(not(feature = "bench-control"), not(feature = "runtime-shaders")))]
#[used]
static EMBEDDED: &str = record!("false", "false");

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct BuildMetadata {
    pub schema: u32,
    pub version: String,
    pub source_commit: String,
    pub dirty: bool,
    pub bench_control: bool,
    pub runtime_shaders: bool,
    pub gpui_rev: String,
    pub rust_version: String,
}
impl BuildMetadata {
    pub fn production(&self, version: &str) -> Result<(), String> {
        if self.schema != 2
            || self.version != version
            || self.source_commit.len() != 40
            || !self.source_commit.bytes().all(|b| b.is_ascii_hexdigit())
            || self.dirty
            || self.bench_control
            || self.gpui_rev != GPUI_REV
            || self.rust_version != RUST_VERSION
        {
            return Err("Native binary provenance mismatch".into());
        }
        Ok(())
    }
}
pub fn current() -> BuildMetadata {
    let bytes = std::hint::black_box(EMBEDDED).as_bytes();
    serde_json::from_slice(&bytes[START.len()..bytes.len() - END.len()])
        .expect("compile-time native metadata")
}

#[cfg(test)]
mod observed_identity_tests {
    #[test]
    fn metadata_records_observed_build_identity() {
        let value = super::current();
        assert_eq!(value.rust_version, env!("TEZBAR_NATIVE_RUST_VERSION"));
        assert_eq!(value.gpui_rev, env!("TEZBAR_NATIVE_GPUI_REV"));
    }
}

/// Memory stays bounded independently of executable size. Only one complete
/// record is allowed; an ambiguous or truncated record is never attested.
pub fn capture(path: &Path) -> Result<(BuildMetadata, String), String> {
    let meta = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !meta.is_file() || meta.file_type().is_symlink() {
        return Err("Native executable must be a regular file".into());
    }
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut pending = Vec::new();
    let mut record = None;
    let mut buffer = [0u8; 65536];
    loop {
        let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        pending.extend_from_slice(&buffer[..n]);
        loop {
            let Some(start) = pending.windows(START.len()).position(|w| w == START) else {
                let keep = pending.len().saturating_sub(START.len());
                pending.drain(..keep);
                break;
            };
            pending.drain(..start);
            if pending.len() <= START.len() {
                break;
            }
            if pending[START.len()] != b'{' {
                pending.drain(..START.len());
                continue;
            }
            let Some(end) = pending[START.len()..]
                .windows(END.len())
                .position(|w| w == END)
            else {
                if pending.len() > RECORD_LIMIT {
                    return Err("Native metadata record too large".into());
                }
                break;
            };
            if end > RECORD_LIMIT || record.is_some() {
                return Err("Ambiguous native binary metadata".into());
            }
            let json = &pending[START.len()..START.len() + end];
            let value = serde_json::from_slice(json).map_err(|e| e.to_string())?;
            record = Some((value, format!("{:x}", Sha256::digest(json))));
            pending.drain(..START.len() + end + END.len());
        }
    }
    if pending.starts_with(START) && pending.get(START.len()) == Some(&b'{') {
        return Err("Truncated native binary metadata".into());
    }
    record.ok_or_else(|| "Native binary metadata missing (requires schema 2 build)".into())
}
