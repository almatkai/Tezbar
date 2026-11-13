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
    let outside = if cfg!(windows) {
        PathBuf::from(r"C:\outside\icon")
    } else {
        PathBuf::from("/outside/icon")
    };
    let outside_uri = url::Url::from_file_path(&outside).unwrap().to_string();
    assert!(resolve(&outside_uri, root, &BTreeSet::new()).is_err());
    assert!(resolve(&outside_uri, root, &[outside].into()).is_ok());
}
#[test]
fn sdk_asset_names_resolve_under_assets_for_every_image_variant() {
    let root = Path::new("/fixture/ext");
    for raw in [
        "pixel.png",
        "./pixel.png",
        "assets/pixel.png",
        "./assets/pixel.png",
    ] {
        let source = resolve(raw, root, &BTreeSet::new()).unwrap();
        assert!(matches!(&source, Source::File(path) if path == &root.join("assets/pixel.png")));
        for variant in [
            Variant::Icon,
            Variant::Avatar,
            Variant::Artwork,
            Variant::Screenshot,
        ] {
            let mut cache = ImageCache::default();
            let (_, effect) = cache.acquire(source.clone(), variant).unwrap();
            assert!(
                matches!(effect, Some(Effect::ReadFile{path,..}) if path == root.join("assets/pixel.png"))
            );
        }
    }
    assert!(
        matches!(resolve("nested/icon.png", root, &BTreeSet::new()),Ok(Source::File(path)) if path == root.join("assets/nested/icon.png"))
    );
    for raw in ["../pixel.png", "assets/../../pixel.png", "./../pixel.png"] {
        assert!(resolve(raw, root, &BTreeSet::new()).is_err());
    }
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
    assert!(
        c.acquire(Source::Builtin("overflow".into()), Variant::Icon)
            .is_err()
    );
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

fn app_effect(effects: Vec<Effect>) -> u64 {
    effects
        .into_iter()
        .find_map(|e| match e {
            Effect::ResolveAppPath { token, .. } => Some(token),
            _ => None,
        })
        .unwrap()
}
fn fallback() -> Option<Source> {
    Some(Source::Remote("https://example.test/catalog".into()))
}
#[test]
fn catalog_selection_clears_resolved_app_before_dispatch() {
    let mut cache = ImageCache::default();
    let mut slot = CatalogIcon::default();
    let token = app_effect(
        slot.select(
            &mut cache,
            "clean",
            Some("/Applications/Clean.app".into()),
            fallback(),
        )
        .unwrap(),
    );
    cache.ready(token, "clean icon".into(), 42);
    assert_eq!(slot.handle(&cache), Some("clean icon"));
    let effects = slot.select(&mut cache, "mole", None, fallback()).unwrap();
    assert!(slot.handle(&cache).is_none());
    assert!(matches!(slot.fallback, Some(Source::Remote(_))));
    assert!(matches!(&effects[0],Effect::Release(h) if h=="clean icon"));
}
#[test]
fn app_icon_scope_includes_extension_and_path() {
    for (extension, path) in [
        ("other", "/Applications/Clean.app"),
        ("clean", "/Applications/Other.app"),
        ("alias", "/Applications/Other.app"),
    ] {
        let mut cache = ImageCache::default();
        let mut slot = CatalogIcon::default();
        let old = app_effect(
            slot.select(
                &mut cache,
                "clean",
                Some("/Applications/Clean.app".into()),
                fallback(),
            )
            .unwrap(),
        );
        cache.ready(old, "old".into(), 42);
        let token = app_effect(
            slot.select(&mut cache, extension, Some(path.into()), fallback())
                .unwrap(),
        );
        assert!(slot.handle(&cache).is_none());
        assert_ne!(token, old);
        cache.ready(token, "current".into(), 42);
        assert_eq!(slot.handle(&cache), Some("current"));
    }
}
#[test]
fn late_app_lookup_before_or_after_current_completion_cannot_replace_selection() {
    for old_first in [true, false] {
        let mut cache = ImageCache::default();
        let mut slot = CatalogIcon::default();
        let old = app_effect(
            slot.select(&mut cache, "a", Some("/a.app".into()), fallback())
                .unwrap(),
        );
        let new = app_effect(
            slot.select(&mut cache, "b", Some("/b.app".into()), fallback())
                .unwrap(),
        );
        if old_first {
            assert_eq!(cache.ready(old, "old".into(), 42).len(), 1);
            assert!(slot.handle(&cache).is_none());
        }
        cache.ready(new, "new".into(), 42);
        if !old_first {
            assert_eq!(cache.ready(old, "old".into(), 42).len(), 1);
        }
        assert_eq!(slot.handle(&cache), Some("new"));
    }
}
#[test]
fn failed_empty_unavailable_app_lookups_keep_current_catalog_fallback() {
    for _outcome in ["empty", "rejected", "unavailable"] {
        let mut cache = ImageCache::default();
        let mut slot = CatalogIcon::default();
        let token = app_effect(
            slot.select(&mut cache, "a", Some("/a.app".into()), fallback())
                .unwrap(),
        );
        cache.failed(token);
        assert!(slot.handle(&cache).is_none());
        assert!(matches!(slot.fallback, Some(Source::Remote(_))));
        assert_eq!(cache.ready(token, "late failed".into(), 42).len(), 1);
    }
}
#[test]
fn app_lookup_after_unmount_is_released() {
    let mut cache = ImageCache::default();
    let mut slot = CatalogIcon::default();
    let token = app_effect(
        slot.select(&mut cache, "a", Some("/a.app".into()), fallback())
            .unwrap(),
    );
    slot.clear(&mut cache);
    assert_eq!(cache.ready(token, "unmounted".into(), 42).len(), 1);
    assert!(slot.handle(&cache).is_none());
    assert!(slot.fallback.is_none());
}
#[test]
fn selected_remote_lease_never_exposes_previous_handle() {
    let mut cache = ImageCache::default();
    let (old, _) = cache.acquire(remote(), Variant::Icon).unwrap();
    cache.ready(old.0, "old".into(), 42);
    cache.release(old);
    let (new, _) = cache
        .acquire(
            Source::Remote("https://example.test/new".into()),
            Variant::Icon,
        )
        .unwrap();
    assert!(cache.peek(new).is_none());
    assert_eq!(cache.ready(old.0, "stale".into(), 42).len(), 1);
    cache.ready(new.0, "new".into(), 42);
    assert_eq!(cache.peek(new), Some("new"));
    cache.release(new);
    assert_eq!(cache.ready(new.0, "after unmount".into(), 42).len(), 1);
}
#[test]
fn failed_or_oversize_image_has_placeholder_and_retries_with_new_token() {
    for oversized in [false, true] {
        let mut cache = ImageCache::default();
        let (old, _) = cache.acquire(remote(), Variant::Icon).unwrap();
        if oversized {
            assert_eq!(
                cache.ready(old.0, "huge".into(), 2 * 1024 * 1024 + 1).len(),
                1
            );
        } else {
            cache.failed(old.0);
        }
        assert!(cache.peek(old).is_none());
        let (new, e) = cache.acquire(remote(), Variant::Icon).unwrap();
        let Effect::Fetch { token, .. } = e.unwrap() else {
            panic!()
        };
        assert_ne!(token, old.0);
        cache.failed(old.0);
        assert_eq!(cache.ready(old.0, "late".into(), 42).len(), 1);
        cache.ready(token, "recovered".into(), 42);
        assert_eq!(cache.peek(old), Some("recovered"));
        assert_eq!(cache.peek(new), Some("recovered"));
    }
}
#[test]
fn windows_drive_and_unc_paths_require_authorization_before_url_parsing() {
    let root = Path::new("/fixture/ext");
    for raw in [
        r"C:\Apps\Icon.png",
        r"C:/Apps/Icon.png",
        r"\\server\share\Icon.png",
    ] {
        assert!(resolve(raw, root, &BTreeSet::new()).is_err());
        let allowed = [PathBuf::from(raw)].into();
        assert!(matches!(resolve(raw, root, &allowed), Ok(Source::File(_))));
    }
    assert!(
        resolve(
            r"C:\Apps\..\secret",
            root,
            &[PathBuf::from(r"C:\Apps\..\secret")].into()
        )
        .is_err()
    );
}

#[test]
fn missing_failed_catalog_icon_recovers_on_different_selection() {
    let mut cache = ImageCache::default();
    let mut slot = CatalogIcon::default();
    slot.select(&mut cache, "missing", None, None).unwrap();
    assert!(slot.handle(&cache).is_none());
    assert!(slot.fallback.is_none());
    let token = app_effect(
        slot.select(&mut cache, "failed", Some("/a.app".into()), None)
            .unwrap(),
    );
    cache.failed(token);
    assert!(slot.handle(&cache).is_none());
    assert!(slot.fallback.is_none());
    slot.select(&mut cache, "remote", None, fallback()).unwrap();
    assert!(matches!(slot.fallback, Some(Source::Remote(_))));
    assert!(slot.handle(&cache).is_none());
}
