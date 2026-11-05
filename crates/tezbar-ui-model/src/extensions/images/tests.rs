use super::*;
fn remote() -> Source {
    Source::Remote("https://example.test/icon".into())
}
#[test]
fn local_data_glyph_and_remote_policy() {
    let root = Path::new("/fixture/ext");
    assert!(matches!(
        resolve("assets/my icon.png", root, &BTreeSet::new()),
        Ok(Source::File(_))
    ));
    assert!(resolve("../secret", root, &BTreeSet::new()).is_err());
    assert!(resolve("http://example.test/icon", root, &BTreeSet::new()).is_err());
    assert!(resolve("https://example.test/icon", root, &BTreeSet::new()).is_ok());
    assert!(matches!(
        resolve("data:image/png;base64,AA==", root, &BTreeSet::new()),
        Ok(Source::Data(_))
    ));
    assert!(resolve("file:///outside/icon", root, &BTreeSet::new()).is_err());
    assert!(resolve(
        "file:///outside/icon",
        root,
        &[PathBuf::from("/outside/icon")].into()
    )
    .is_ok());
}
#[test]
fn coalesces_inflight_and_peeks_only_owned_ready_handles() {
    let mut c = ImageCache::default();
    let (a, e) = c.acquire(remote(), Variant::Icon).unwrap();
    let Effect::Fetch { token, .. } = e.unwrap() else {
        panic!()
    };
    let (b, e) = c.acquire(remote(), Variant::Icon).unwrap();
    assert!(e.is_none());
    assert!(c.peek(a).is_none());
    assert!(c.ready(token, "handle".into(), 42).is_empty());
    assert_eq!(c.peek(b), Some("handle"));
    assert!(c.release(a).is_empty());
    assert_eq!(c.release(b).len(), 1);
    assert!(c.peek(b).is_none());
    assert!(c.release(b).is_empty());
}
#[test]
fn late_image_cannot_replace_new_acquisition() {
    let mut c = ImageCache::default();
    let (a, e) = c.acquire(remote(), Variant::Icon).unwrap();
    let Effect::Fetch { token, .. } = e.unwrap() else {
        panic!()
    };
    c.release(a);
    let (b, _) = c.acquire(remote(), Variant::Icon).unwrap();
    assert_eq!(c.ready(token, "late".into(), 42).len(), 1);
    assert!(c.peek(b).is_none());
}
#[test]
fn variant_isolation_and_idle_releases_all() {
    let mut c = ImageCache::default();
    let (a, _) = c.acquire(remote(), Variant::Icon).unwrap();
    let (b, e) = c.acquire(remote(), Variant::Avatar).unwrap();
    assert!(e.is_some());
    c.ready(a.0, "icon".into(), 42);
    assert!(c.peek(b).is_none());
    assert_eq!(c.idle().len(), 1);
    assert_eq!(c.retained_bytes(), 0);
}
#[test]
fn oversized_resource_rejected_and_cache_bounded() {
    let mut c = ImageCache::default();
    let (a, _) = c.acquire(remote(), Variant::Icon).unwrap();
    assert_eq!(c.ready(a.0, "huge".into(), 2 * 1024 * 1024 + 1).len(), 1);
    assert!(c.peek(a).is_none());
    for i in 1..128 {
        c.acquire(Source::Builtin(format!("icon{i}")), Variant::Icon)
            .unwrap();
    }
    assert!(c
        .acquire(Source::Builtin("overflow".into()), Variant::Icon)
        .is_err());
}
#[test]
fn one_hundred_mount_hide_cycles_release_image_handles() {
    let mut c = ImageCache::default();
    for _ in 0..100 {
        let (lease, effect) = c.acquire(remote(), Variant::Screenshot).unwrap();
        let Some(Effect::Fetch { token, .. }) = effect else {
            panic!()
        };
        assert!(c.ready(token, "owned".into(), 1024).is_empty());
        assert_eq!(c.peek(lease), Some("owned"));
        assert_eq!(c.idle().len(), 1);
        assert_eq!(c.retained_bytes(), 0);
    }
}
