//! Native asset capabilities. No UI types, runtime creation, cache, or eager HTTP client.
//! Dropping a read/fetch future drops its response/file and releases its admission slot.
use base64::{engine::general_purpose::STANDARD, Engine};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
    time::Duration,
};
use tokio::io::AsyncReadExt;

pub const ASSET_BYTES: usize = 2 * 1024 * 1024;
pub const CHAT_SOURCE_BYTES: usize = 8 * 1024 * 1024;
pub const DEADLINE: Duration = Duration::from_secs(10);
const FAILURE: &str = "Image could not be loaded";

/// Only the canonical installed registry may issue this capability. Never deserialized.
#[derive(Clone)]
pub struct AssetScope {
    root: PathBuf,
    legacy: BTreeSet<PathBuf>,
}
impl AssetScope {
    pub(crate) fn registered(root: PathBuf, legacy: BTreeSet<PathBuf>) -> Self {
        Self { root, legacy }
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn allowed_legacy(&self) -> &BTreeSet<PathBuf> {
        &self.legacy
    }
    async fn authorize(&self, file: &Path) -> Result<PathBuf, String> {
        let root = tokio::fs::canonicalize(&self.root)
            .await
            .map_err(|_| FAILURE)?;
        // An issued root cannot be replaced by a symlink after admission.
        if root != self.root {
            return Err(FAILURE.into());
        }
        let canonical = tokio::fs::canonicalize(file).await.map_err(|_| FAILURE)?;
        if canonical.starts_with(&root) || (self.legacy.contains(file) && canonical == file) {
            Ok(canonical)
        } else {
            Err("Image is outside the installed extension".into())
        }
    }
}

/// One owner in NativeCore. All consumers share the same finite admission and client.
pub struct AssetService {
    client: OnceLock<Result<reqwest::Client, ()>>,
    slots: Arc<tokio::sync::Semaphore>,
}
impl Default for AssetService {
    fn default() -> Self {
        Self {
            client: OnceLock::new(),
            slots: Arc::new(tokio::sync::Semaphore::new(4)),
        }
    }
}
impl AssetService {
    /// Also bounds off-thread decodes. Keep the permit inside the blocking job so a
    /// cancelled waiter cannot admit an unbounded number of still-running decodes.
    pub async fn decode_permit(&self) -> Result<tokio::sync::OwnedSemaphorePermit, String> {
        self.slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| "Image service is busy".into())
    }
    fn client(&self) -> Result<&reqwest::Client, String> {
        self.client
            .get_or_init(|| {
                reqwest::Client::builder()
                    .timeout(DEADLINE)
                    .connect_timeout(DEADLINE)
                    .redirect(reqwest::redirect::Policy::none())
                    // Refuse compressed HTTP rather than expanding an unbounded body.
                    .no_gzip()
                    .no_brotli()
                    .no_deflate()
                    .no_zstd()
                    .build()
                    .map_err(|_| ())
            })
            .as_ref()
            .map_err(|_| FAILURE.to_string())
    }

    pub async fn remote(&self, raw: &str, limit: usize) -> Result<Vec<u8>, String> {
        check_limit(limit)?;
        let mut url = https(raw)?;
        let _permit = self.decode_permit().await?;
        tokio::time::timeout(DEADLINE, async {
            for redirects in 0..=4 {
                let mut response = self
                    .client()?
                    .get(url.clone())
                    .header(reqwest::header::ACCEPT_ENCODING, "identity")
                    .send()
                    .await
                    .map_err(|_| FAILURE)?;
                if response.status().is_redirection() {
                    if redirects == 4 {
                        return Err("Too many image redirects".into());
                    }
                    let location = response
                        .headers()
                        .get(reqwest::header::LOCATION)
                        .and_then(|v| v.to_str().ok())
                        .ok_or(FAILURE)?;
                    url = redirect(&url, location)?;
                    continue;
                }
                response_policy(
                    response.status().as_u16(),
                    response
                        .headers()
                        .get(reqwest::header::CONTENT_ENCODING)
                        .map(|v| v.as_bytes()),
                    response.content_length(),
                    limit,
                )?;
                return collect(response.bytes_stream(), limit).await;
            }
            Err(FAILURE.into())
        })
        .await
        .map_err(|_| "Image request timed out".to_string())?
    }
    pub async fn local(
        &self,
        scope: &AssetScope,
        file: &Path,
        limit: usize,
    ) -> Result<Vec<u8>, String> {
        check_limit(limit)?;
        let permit = self.decode_permit().await?;
        tokio::time::timeout(DEADLINE, async {
            let canonical = scope.authorize(file).await?;
            let scope = scope.clone();
            tokio::task::spawn_blocking(move || {
                let _permit = permit;
                read_scoped(&scope, &canonical, limit)
            })
            .await
            .map_err(|_| FAILURE.to_string())?
        })
        .await
        .map_err(|_| "Image read timed out".to_string())?
    }
    /// Host file-dialog results only. Never exposed through extension serialized props.
    pub async fn chosen_image(&self, path: &Path) -> Result<Vec<u8>, String> {
        let _permit = self.decode_permit().await?;
        tokio::time::timeout(DEADLINE, read(path, CHAT_SOURCE_BYTES))
            .await
            .map_err(|_| "Image read timed out".to_string())?
    }
}
// Descriptor traversal prevents a concurrent extension symlink swap from widening scope.
fn read_scoped(scope: &AssetScope, canonical: &Path, limit: usize) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let (root, relative) = if let Ok(relative) = canonical.strip_prefix(&scope.root) {
        (scope.root.clone(), relative.to_path_buf())
    } else if scope.legacy.contains(canonical) {
        (
            canonical.parent().ok_or(FAILURE)?.to_path_buf(),
            PathBuf::from(canonical.file_name().ok_or(FAILURE)?),
        )
    } else {
        return Err(FAILURE.into());
    };
    #[cfg(unix)]
    let file = {
        use std::{
            ffi::CString,
            os::{
                fd::{AsRawFd, FromRawFd},
                unix::fs::OpenOptionsExt,
            },
        };
        let mut dir = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open("/")
            .map_err(|_| FAILURE)?;
        // Traverse the issued root itself as well: checking just its final
        // component would still follow a swapped symlink in an ancestor.
        let all = root
            .components()
            .chain(relative.components())
            .collect::<Vec<_>>();
        if !root.is_absolute()
            || all.iter().any(|c| {
                !matches!(
                    c,
                    std::path::Component::RootDir | std::path::Component::Normal(_)
                )
            })
        {
            return Err(FAILURE.into());
        }
        let components = all
            .into_iter()
            .filter(|c| !matches!(c, std::path::Component::RootDir))
            .collect::<Vec<_>>();
        if components.is_empty() {
            return Err(FAILURE.into());
        }
        for (index, component) in components.iter().enumerate() {
            let std::path::Component::Normal(name) = component else {
                return Err(FAILURE.into());
            };
            use std::os::unix::ffi::OsStrExt;
            let name = CString::new(name.as_bytes()).map_err(|_| FAILURE)?;
            let flags = libc::O_RDONLY
                | libc::O_CLOEXEC
                | libc::O_NOFOLLOW
                | libc::O_NONBLOCK
                | if index + 1 < components.len() {
                    libc::O_DIRECTORY
                } else {
                    0
                };
            let fd = unsafe { libc::openat(dir.as_raw_fd(), name.as_ptr(), flags) };
            if fd < 0 {
                return Err(FAILURE.into());
            }
            dir = unsafe { std::fs::File::from_raw_fd(fd) };
        }
        dir
    };
    #[cfg(not(unix))]
    let file = {
        // Reject reparse/symlink paths at each read. Windows device acceptance is separate.
        let mut current = root;
        for part in relative.components() {
            current.push(part);
            if std::fs::symlink_metadata(&current)
                .map_err(|_| FAILURE)?
                .file_type()
                .is_symlink()
            {
                return Err(FAILURE.into());
            }
        }
        std::fs::File::open(current).map_err(|_| FAILURE)?
    };
    let metadata = file.metadata().map_err(|_| FAILURE)?;
    if !metadata.is_file() || metadata.len() > limit as u64 {
        return Err(FAILURE.into());
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| FAILURE)?;
    if bytes.len() > limit {
        return Err(FAILURE.into());
    }
    Ok(bytes)
}
fn response_policy(
    status: u16,
    encoding: Option<&[u8]>,
    length: Option<u64>,
    limit: usize,
) -> Result<(), String> {
    if !(200..300).contains(&status)
        || encoding.is_some_and(|v| v != b"identity")
        || length.is_some_and(|n| n > limit as u64)
    {
        Err(FAILURE.into())
    } else {
        Ok(())
    }
}
async fn collect<S, B, E>(stream: S, limit: usize) -> Result<Vec<u8>, String>
where
    S: futures_util::Stream<Item = Result<B, E>>,
    B: AsRef<[u8]>,
{
    use futures_util::StreamExt;
    futures_util::pin_mut!(stream);
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| FAILURE)?;
        let chunk = chunk.as_ref();
        if chunk.len() > limit.saturating_sub(bytes.len()) {
            return Err("Image exceeds source limit".into());
        }
        bytes.extend_from_slice(chunk);
    }
    Ok(bytes)
}
fn check_limit(limit: usize) -> Result<(), String> {
    if limit == 0 || limit > CHAT_SOURCE_BYTES {
        Err("Invalid image byte bound".into())
    } else {
        Ok(())
    }
}
async fn read(path: &Path, limit: usize) -> Result<Vec<u8>, String> {
    let path = tokio::fs::canonicalize(path).await.map_err(|_| FAILURE)?;
    let file = tokio::fs::File::open(path).await.map_err(|_| FAILURE)?;
    let metadata = file.metadata().await.map_err(|_| FAILURE)?;
    if !metadata.is_file() || metadata.len() > limit as u64 {
        return Err("Image exceeds source limit".into());
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(|_| FAILURE)?;
    if bytes.len() > limit {
        return Err("Image exceeds source limit".into());
    }
    Ok(bytes)
}
fn https(raw: &str) -> Result<reqwest::Url, String> {
    let url = reqwest::Url::parse(raw).map_err(|_| FAILURE)?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.host_str().is_none()
    {
        return Err("Image URL must be HTTPS without credentials".into());
    }
    Ok(url)
}
fn redirect(base: &reqwest::Url, location: &str) -> Result<reqwest::Url, String> {
    https(base.join(location).map_err(|_| FAILURE)?.as_str())
}
/// Strict MIME and STANDARD base64, including padding/unused-bit validation.
pub fn data_uri(raw: &str, limit: usize) -> Result<Vec<u8>, String> {
    check_limit(limit)?;
    if raw.len() > limit.div_ceil(3) * 4 + 64 {
        return Err("Image exceeds source limit".into());
    }
    let (header, payload) = raw.split_once(',').ok_or(FAILURE)?;
    if !matches!(
        header,
        "data:image/png;base64"
            | "data:image/jpeg;base64"
            | "data:image/webp;base64"
            | "data:image/gif;base64"
            | "data:image/tiff;base64"
            | "data:image/svg+xml;base64"
    ) {
        return Err("Unsupported image data MIME".into());
    }
    let bytes = STANDARD.decode(payload).map_err(|_| FAILURE)?;
    if bytes.len() > limit {
        return Err("Image exceeds source limit".into());
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn client_is_lazy_and_url_redirect_policy_is_strict() {
        let service = AssetService::default();
        assert!(service.client.get().is_none());
        let base = https("https://fixture.invalid/image").unwrap();
        assert!(redirect(&base, "/other").is_ok());
        for target in [
            "http://fixture.invalid/image",
            "https://name:secret@fixture.invalid/",
            "file:///image",
        ] {
            assert!(redirect(&base, target).is_err());
        }
        assert!(service.client.get().is_none());
    }
    #[test]
    fn strict_data_mime_and_full_byte_bound() {
        assert_eq!(data_uri("data:image/png;base64,AQ==", 1).unwrap(), [1]);
        for value in [
            "data:image/png;base64,AR==",
            "data:image/png;base64,AQ",
            "data:text/html;base64,AQ==",
            "data:image/png,abc",
        ] {
            assert!(data_uri(value, 10).is_err());
        }
        assert!(data_uri("data:image/png;base64,AQI=", 1).is_err());
    }
    #[tokio::test]
    async fn status_encoding_and_stream_are_bounded_without_network() {
        assert!(response_policy(200, None, None, 3).is_ok());
        for (status, encoding, length) in [
            (404, None, None),
            (200, Some(b"gzip".as_slice()), None),
            (200, None, Some(4)),
        ] {
            assert!(response_policy(status, encoding, length, 3).is_err());
        }
        let chunks = || futures_util::stream::iter([Ok::<_, ()>(vec![1, 2]), Ok(vec![3, 4])]);
        assert_eq!(collect(chunks(), 4).await.unwrap(), [1, 2, 3, 4]);
        assert!(collect(chunks(), 3).await.is_err());
        let failed = futures_util::stream::iter([Ok(vec![1]), Err(())]);
        assert!(collect(failed, 8).await.is_err());
    }
    #[tokio::test]
    async fn dropping_body_wait_drops_stream_and_releases_slot() {
        use std::sync::atomic::{AtomicBool, Ordering};
        struct Held(Arc<AtomicBool>);
        impl Drop for Held {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        impl futures_util::Stream for Held {
            type Item = Result<Vec<u8>, ()>;
            fn poll_next(
                self: std::pin::Pin<&mut Self>,
                _: &mut std::task::Context<'_>,
            ) -> std::task::Poll<Option<Self::Item>> {
                std::task::Poll::Pending
            }
        }
        let service = AssetService::default();
        let dropped = Arc::new(AtomicBool::new(false));
        let permit = service.decode_permit().await.unwrap();
        let seen = dropped.clone();
        let job = tokio::spawn(async move {
            let _permit = permit;
            collect(Held(seen), 8).await
        });
        tokio::task::yield_now().await;
        job.abort();
        let _ = job.await;
        assert!(dropped.load(Ordering::SeqCst));
        assert_eq!(service.slots.available_permits(), 4);
    }
    #[tokio::test]
    async fn file_bound_and_cancelled_admission_release() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("image");
        std::fs::write(&path, [1, 2, 3]).unwrap();
        let scope = AssetScope::registered(root.path().canonicalize().unwrap(), BTreeSet::new());
        let service = AssetService::default();
        assert_eq!(service.local(&scope, &path, 3).await.unwrap(), [1, 2, 3]);
        assert!(service.local(&scope, &path, 2).await.is_err());
        let permits = (0..4).map(|_| service.decode_permit()).collect::<Vec<_>>();
        let mut owned = Vec::new();
        for permit in permits {
            owned.push(permit.await.unwrap());
        }
        assert!(service.decode_permit().await.is_err());
        drop(owned);
        assert!(service.decode_permit().await.is_ok());
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn symlink_escape_requires_exact_registry_grant() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let secret = outside.path().join("image");
        std::fs::write(&secret, [1]).unwrap();
        let link = root.path().join("image");
        std::os::unix::fs::symlink(&secret, &link).unwrap();
        let scope = AssetScope::registered(root.path().canonicalize().unwrap(), BTreeSet::new());
        let service = AssetService::default();
        assert!(service.local(&scope, &link, 8).await.is_err());
        let secret = secret.canonicalize().unwrap();
        let scope = AssetScope::registered(scope.root, [secret.clone()].into());
        assert_eq!(service.local(&scope, &secret, 8).await.unwrap(), [1]);
        assert!(service.local(&scope, &link, 8).await.is_err());
    }
}

#[cfg(test)]
mod race_tests {
    use super::*;
    #[cfg(unix)]
    #[test]
    fn root_ancestor_swap_after_authorization_cannot_redirect_scoped_open() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let parent = root.path().canonicalize().unwrap().join("parent");
        let package = parent.join("package");
        std::fs::create_dir_all(&package).unwrap();
        let file = package.join("image");
        std::fs::write(&file, [1]).unwrap();
        let scope = AssetScope::registered(package, BTreeSet::new());
        let replacement = outside.path().canonicalize().unwrap();
        std::fs::create_dir(replacement.join("package")).unwrap();
        std::fs::write(replacement.join("package/image"), [2]).unwrap();
        std::fs::rename(&parent, root.path().join("original")).unwrap();
        std::os::unix::fs::symlink(replacement, &parent).unwrap();
        assert!(read_scoped(&scope, &file, 8).is_err());
    }
    #[test]
    fn removed_or_symlink_replaced_after_authorization_is_denied_at_open() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let scope = AssetScope::registered(root.path().canonicalize().unwrap(), BTreeSet::new());
        let path = scope.root().join("asset");
        std::fs::write(&path, [1]).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert!(read_scoped(&scope, &path, 8).is_err());
        #[cfg(unix)]
        {
            let secret = outside.path().join("secret");
            std::fs::write(&secret, [2]).unwrap();
            std::os::unix::fs::symlink(&secret, &path).unwrap();
            assert!(read_scoped(&scope, &path, 8).is_err());
            std::fs::remove_file(&path).unwrap();
            let dir = scope.root().join("nested");
            std::fs::create_dir(&dir).unwrap();
            std::fs::remove_dir(&dir).unwrap();
            std::os::unix::fs::symlink(outside.path(), &dir).unwrap();
            assert!(read_scoped(&scope, &dir.join("secret"), 8).is_err());
        }
    }
}
