//! Host catalog routing metadata, deliberately outside hash-bound release policy.
use serde_json::{json, Value};
use std::{collections::HashMap, sync::OnceLock};

pub(super) const LOCAL_GUIDANCE: &str = "Import the trusted release manifest and ZIP with --extensions-v3-import, then close and reopen Tezbar. Use local import again to update or repair.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Rule {
    Remote,
    LocalImportOnly,
}

pub(super) fn parse(value: Option<&Value>) -> Result<Rule, String> {
    match value {
        None => Ok(Rule::Remote),
        Some(Value::String(s)) if s == "remote" => Ok(Rule::Remote),
        Some(Value::String(s)) if s == "local-import-only" => Ok(Rule::LocalImportOnly),
        _ => Err("Unsupported catalog acquisition rule; remote installation disabled".into()),
    }
}

// Mirror the repository-root grammar in shared/extensionRepository.ts. Empty
// query/fragment markers are equivalent to absence (JS search/hash are empty).
// URL parsing handles host case, whitespace and the default HTTPS port; IDs keep
// their case-sensitive identity and terminal-slash behavior after JS trim.
fn trim_reference(reference: &str) -> &str {
    // ECMAScript trim also removes BOM; Rust trim additionally removes NEL.
    reference.trim_matches(|c: char| (c.is_whitespace() && c != '\u{0085}') || c == '\u{feff}')
}

fn repository_key(reference: &str) -> Option<String> {
    let root = trim_reference(reference);
    let url = reqwest::Url::parse(root).ok()?;
    if url.scheme() != "https"
        || !matches!(url.host_str(), Some("github.com" | "www.github.com"))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.query().is_some_and(|query| !query.is_empty())
        || url.fragment().is_some_and(|fragment| !fragment.is_empty())
    {
        return None;
    }
    let mut segments = url.path().split('/').filter(|s| !s.is_empty());
    let owner = segments.next()?;
    let repository = segments.next()?;
    if segments.next().is_some() {
        return None;
    }
    let repository = if repository.to_ascii_lowercase().ends_with(".git") {
        &repository[..repository.len() - 4]
    } else {
        repository
    };
    if owner.len() > 39
        || !owner.as_bytes().first()?.is_ascii_alphanumeric()
        || !owner.as_bytes().last()?.is_ascii_alphanumeric()
        || !owner
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-')
        || repository.is_empty()
        || !repository
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-'))
    {
        return None;
    }
    Some(format!(
        "https://github.com/{}/{}",
        owner.to_ascii_lowercase(),
        repository.to_ascii_lowercase()
    ))
}

fn reference_key(reference: &str) -> String {
    repository_key(reference)
        .unwrap_or_else(|| trim_reference(reference).trim_end_matches('/').to_owned())
}

fn insert_rule(
    rules: &mut HashMap<String, Value>,
    reference: &str,
    rule: &Value,
) -> Result<(), String> {
    let key = reference_key(reference);
    if rules.get(&key).is_some_and(|existing| existing != rule) {
        return Err("Conflicting catalog acquisition rules; remote installation disabled".into());
    }
    rules.insert(key, rule.clone());
    Ok(())
}

fn expand_rules(raw: &Value, manifests: &[Value]) -> Result<HashMap<String, Value>, String> {
    let mut configured = HashMap::new();
    for (id, rule) in raw
        .as_object()
        .ok_or("Invalid catalog acquisition metadata")?
    {
        insert_rule(&mut configured, id, rule)?;
    }
    let mut rules = configured.clone();
    // Declared legacy identities and equivalent repository roots share the
    // configured rule. Conflicting aliases fail closed, independent of order.
    for manifest in manifests {
        let Some(rule) = manifest["id"]
            .as_str()
            .and_then(|id| configured.get(&reference_key(id)))
        else {
            continue;
        };
        for alias in manifest["replaces"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .chain(manifest["repository"].as_str())
        {
            insert_rule(&mut rules, alias, rule)?;
        }
    }
    Ok(rules)
}

pub(super) fn for_id(id: &str) -> Result<Rule, String> {
    // Catalog searches may contain thousands of legacy entries. Expand routing
    // aliases once, rather than parsing release manifests for every result row.
    static RULES: OnceLock<Result<HashMap<String, Value>, String>> = OnceLock::new();
    let rules = RULES
        .get_or_init(|| {
            let raw: Value = serde_json::from_str(include_str!("acquisition.json"))
                .map_err(|_| "Invalid catalog acquisition metadata")?;
            expand_rules(&raw, &super::v2_catalog::source())
        })
        .as_ref()
        .map_err(Clone::clone)?;
    parse(rules.get(&reference_key(id)))
}

pub(super) fn require_remote(rule: Result<Rule, String>) -> Result<(), String> {
    match rule? {
        Rule::Remote => Ok(()),
        Rule::LocalImportOnly => Err(LOCAL_GUIDANCE.into()),
    }
}

pub(super) fn metadata(rule: Result<Rule, String>) -> Value {
    match rule {
        Ok(Rule::Remote) => json!({"type":"remote"}),
        Ok(Rule::LocalImportOnly) => json!({"type":"local-import-only","guidance":LOCAL_GUIDANCE}),
        Err(error) => json!({"type":"unsupported","guidance":error}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unknown_or_malformed_rules_never_enable_remote_install() {
        assert_eq!(parse(None).unwrap(), Rule::Remote);
        assert_eq!(parse(Some(&json!("remote"))).unwrap(), Rule::Remote);
        for value in [
            json!(null),
            json!({}),
            json!([]),
            json!(true),
            json!("unknown"),
        ] {
            assert!(require_remote(parse(Some(&value))).is_err());
            assert_eq!(metadata(parse(Some(&value)))["type"], "unsupported");
        }
        assert!(require_remote(parse(Some(&json!("local-import-only")))).is_err());
    }
    #[test]
    fn declared_aliases_and_repository_share_local_acquisition() {
        for manifest in super::super::v2_catalog::source() {
            let id = manifest["id"].as_str().unwrap();
            if for_id(id) == Ok(Rule::LocalImportOnly) {
                for alias in manifest["replaces"].as_array().unwrap() {
                    assert_eq!(
                        for_id(alias.as_str().unwrap()).unwrap(),
                        Rule::LocalImportOnly
                    );
                }
                assert_eq!(
                    for_id(manifest["repository"].as_str().unwrap()).unwrap(),
                    Rule::LocalImportOnly
                );
            }
        }
    }
    const EQUIVALENT_PRIVATE_ROOTS: &[&str] = &[
        "https://github.com/almatkai/port-manager",
        "https://github.com/Almatkai/port-manager",
        "https://github.com/almatkai/Port-Manager",
        "https://github.com/ALMATKAI/PORT-MANAGER",
        "https://www.github.com/Almatkai/Port-Manager",
        "HTTPS://GITHUB.COM/Almatkai/Port-Manager",
        "https://github.com/almatkai/port-manager.git",
        "https://github.com/almatkai/port-manager.GiT///",
        " \tHTTPS://WWW.GITHUB.COM/ALMATKAI/PORT-MANAGER.GiT///\r\n",
        "https://github.com:443/Almatkai/Port-Manager.git/",
        "\u{feff}https://github.com/Almatkai/Port-Manager\u{00a0}",
        "https://www.github.com:0443//Almatkai///Port-Manager//",
        "https://github.com/almatkai/other/../port-manager",
    ];

    // Each base is independently accepted by the downstream repository parser.
    // Exercise empty markers before the UI has a chance to canonicalize them.
    const EMPTY_MARKER_PRIVATE_ROOTS: &[&str] = &[
        "https://github.com/almatkai/port-manager",
        "HTTPS://GITHUB.COM/Almatkai/Port-Manager",
        "https://www.github.com/ALMATKAI/PORT-MANAGER",
        "https://github.com/almatkai/port-manager.GiT///",
        "https://github.com:443/Almatkai/Port-Manager.git/",
        "https://www.github.com:0443//Almatkai///Port-Manager.GiT//",
        "https://github.com/almatkai/other/../port-manager",
    ];

    fn empty_marker_private_references() -> impl Iterator<Item = String> {
        EMPTY_MARKER_PRIVATE_ROOTS.iter().flat_map(|root| {
            ["?", "#", "?#"]
                .into_iter()
                .map(move |marker| format!("{root}{marker}"))
        })
    }

    const NON_EQUIVALENT_REFERENCES: &[&str] = &[
        "http://github.com/almatkai/port-manager",
        "https://github.com:444/almatkai/port-manager",
        "https://github.com.evil.test/almatkai/port-manager",
        "https://evil.test/github.com/almatkai/port-manager",
        "https://github.com/almatkai/other-repository",
        "https://github.com/another-owner/port-manager",
        "https://user@github.com/almatkai/port-manager",
        "https://user:password@github.com/almatkai/port-manager",
        "https://github.com/almatkai/port-manager?tab=readme",
        "https://github.com/almatkai/port-manager#readme",
        "https://github.com/almatkai/port-manager?#readme",
        "https://github.com/almatkai/port-manager?tab=readme#",
        "https://github.com/almatkai/port-manager?%20",
        "https://github.com/almatkai/port-manager#%20",
        "https://github.com/almatkai/port-manager/issues",
        "https://github.com/almatkai/port-manager.git/extra",
        "https://github.com/almatkai%2Fother/port-manager",
        "https://github.com/-almatkai/port-manager",
        "https://github.com/almatkai-/port-manager",
        "https://github.com/almatkai/.git",
        "https://github.com/almatkai/port%2Dmanager",
        "COM.TEZBAR.NATIVE.PORT-MANAGER",
        "raycast.Port-Manager",
        " \t\r\n",
        "\u{feff}\u{00a0}",
        "\u{0085}raycast.port-manager\u{0085}",
        "\u{0085}com.tezbar.native.port-manager\u{0085}",
        " raycast.Port-Manager ",
        " COM.TEZBAR.NATIVE.PORT-MANAGER ",
        " \t raycast.unrelated \r\n",
        "\u{feff}com.tezbar.native.unrelated\u{00a0}",
    ];

    #[test]
    fn equivalent_repository_roots_share_local_acquisition() {
        for reference in EQUIVALENT_PRIVATE_ROOTS {
            assert_eq!(
                for_id(reference),
                Ok(Rule::LocalImportOnly),
                "{reference:?}"
            );
        }
        for reference in empty_marker_private_references() {
            assert_eq!(
                for_id(&reference),
                Ok(Rule::LocalImportOnly),
                "{reference:?}"
            );
        }
        for reference in NON_EQUIVALENT_REFERENCES {
            assert_eq!(for_id(reference), Ok(Rule::Remote), "{reference:?}");
        }
        // Preserve existing ID behavior, including terminal slashes, without
        // making IDs case-insensitive.
        for id in [
            "com.tezbar.native.port-manager",
            "raycast.port-manager",
            "com.tezbar.port-manager",
            "raycast.port-manager/",
        ] {
            assert_eq!(for_id(id), Ok(Rule::LocalImportOnly), "{id}");
        }
    }

    #[test]
    fn normalization_applies_to_trusted_insertion_and_conflicts_fail_closed() {
        let repository = "https://github.com/example/project";
        let manifest = json!({
            "id":"com.tezbar.native.example",
            "repository":" HTTPS://WWW.GITHUB.COM:443/Example/Project.GIT/?# ",
            "replaces":["raycast.Example"]
        });
        let local = json!({"com.tezbar.native.example":"local-import-only"});
        let rules = expand_rules(&local, &[manifest.clone()]).unwrap();
        assert_eq!(parse(rules.get(repository)), Ok(Rule::LocalImportOnly));
        assert_eq!(
            parse(rules.get("raycast.Example")),
            Ok(Rule::LocalImportOnly)
        );
        assert_eq!(parse(rules.get("raycast.example")), Ok(Rule::Remote));
        let direct =
            json!({" HTTPS://WWW.GITHUB.COM:443/Example/Project.GIT/?# ":"local-import-only"});
        assert_eq!(
            parse(expand_rules(&direct, &[]).unwrap().get(repository)),
            Ok(Rule::LocalImportOnly)
        );
        for (left, right) in [
            ("remote", "local-import-only"),
            ("local-import-only", "remote"),
            ("unsupported", "local-import-only"),
        ] {
            let raw = json!({
                "https://github.com/example/project":left,
                "https://www.github.com/EXAMPLE/PROJECT.git/?#":right
            });
            assert!(expand_rules(&raw, &[]).unwrap_err().contains("Conflicting"));
        }
        let raw = json!({
            "com.tezbar.native.example":"local-import-only",
            "https://github.com/example/project":"remote"
        });
        assert!(expand_rules(&raw, &[manifest.clone()]).is_err());
        let other = json!({"id":"com.tezbar.native.other", "repository":repository});
        let raw = json!({"com.tezbar.native.example":"local-import-only", "com.tezbar.native.other":"remote"});
        for manifests in [vec![manifest.clone(), other.clone()], vec![other, manifest]] {
            assert!(expand_rules(&raw, &manifests).is_err());
        }
    }

    #[test]
    fn js_whitespace_id_insertion_and_alias_conflicts_fail_closed() {
        let manifest = json!({
            "id":" \u{feff}com.tezbar.native.example\u{00a0} ",
            "replaces":["\u{00a0}raycast.Example\u{feff}"],
            "repository":"https://github.com/example/project"
        });
        let raw = json!({"\u{feff}com.tezbar.native.example\u{00a0}":"local-import-only"});
        let rules = expand_rules(&raw, &[manifest.clone()]).unwrap();
        for id in ["com.tezbar.native.example", "raycast.Example"] {
            assert_eq!(parse(rules.get(id)), Ok(Rule::LocalImportOnly), "{id}");
        }
        assert_eq!(parse(rules.get("raycast.example")), Ok(Rule::Remote));
        for (left, right) in [
            ("remote", "local-import-only"),
            ("local-import-only", "remote"),
            ("unsupported", "local-import-only"),
        ] {
            let raw = json!({
                "raycast.Example":left,
                " \t\u{feff}raycast.Example\u{00a0}\r\n":right
            });
            assert!(expand_rules(&raw, &[]).unwrap_err().contains("Conflicting"));
        }
        let raw = json!({
            "com.tezbar.native.example":"local-import-only",
            " raycast.Example ":"remote"
        });
        assert!(expand_rules(&raw, &[manifest.clone()])
            .unwrap_err()
            .contains("Conflicting"));
        let other = json!({"id":"com.tezbar.native.other", "replaces":["raycast.Example"]});
        let raw = json!({"com.tezbar.native.example":"local-import-only", "com.tezbar.native.other":"remote"});
        for manifests in [vec![manifest.clone(), other.clone()], vec![other, manifest]] {
            assert!(expand_rules(&raw, &manifests)
                .unwrap_err()
                .contains("Conflicting"));
        }
    }

    fn wrapped_private_ids() -> impl Iterator<Item = String> {
        [
            "com.tezbar.native.port-manager",
            "raycast.port-manager",
            "com.tezbar.port-manager",
        ]
        .into_iter()
        .flat_map(|id| {
            [
                (" \t", "\r\n "),
                ("\u{feff}", "\u{feff}"),
                ("\u{00a0}", "\u{00a0}"),
                ("\u{feff} \t", "\u{00a0}\r\n"),
            ]
            .into_iter()
            .map(move |(left, right)| format!("{left}{id}{right}"))
        })
    }

    #[test]
    fn surrounding_js_whitespace_preserves_declared_id_acquisition() {
        for reference in wrapped_private_ids() {
            assert_eq!(
                for_id(&reference),
                Ok(Rule::LocalImportOnly),
                "{reference:?}"
            );
        }
    }

    fn captured_core() -> (
        tempfile::TempDir,
        super::super::ExtensionsCore,
        std::sync::Arc<std::sync::Mutex<Vec<(String, Vec<Value>)>>>,
    ) {
        use crate::native_core::common::{CoreContext, CorePaths};
        use std::sync::{Arc, Mutex};
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path();
        let ctx = CoreContext::headless(CorePaths {
            data_dir: path.into(),
            config_dir: path.join("config"),
            resource_dir: path.join("resources"),
            temp_dir: path.join("tmp"),
            home_dir: path.into(),
        });
        let calls = Arc::new(Mutex::new(Vec::new()));
        let captured = calls.clone();
        let mut core = super::super::ExtensionsCore::new(ctx);
        core.worker_override = Some(Arc::new(move |channel, args| {
            captured.lock().unwrap().push((channel, args));
            Box::pin(async { Ok(json!({"worker":"captured"})) })
        }));
        (temp, core, calls)
    }

    #[tokio::test]
    async fn repository_install_aliases_block_before_worker_dispatch() {
        let (_temp, core, calls) = captured_core();
        for channel in [
            "extension:install",
            "extensions:install",
            "extensions:reinstall",
        ] {
            for reference in EQUIVALENT_PRIVATE_ROOTS.iter().copied().chain([
                "com.tezbar.native.port-manager",
                "raycast.port-manager",
                "com.tezbar.port-manager",
            ]) {
                let result = core.handle(channel, &[json!(reference)]).await.unwrap();
                assert_eq!(
                    result,
                    Err(LOCAL_GUIDANCE.into()),
                    "{channel}: {reference:?}"
                );
            }
        }
        assert!(
            calls.lock().unwrap().is_empty(),
            "private acquisition reached worker"
        );
        assert!(!core.worker_running().await);
    }

    #[tokio::test]
    async fn empty_repository_markers_block_before_worker_dispatch() {
        let (_temp, core, calls) = captured_core();
        for channel in [
            "extension:install",
            "extensions:install",
            "extensions:reinstall",
        ] {
            for reference in empty_marker_private_references() {
                let result = core.handle(channel, &[json!(&reference)]).await.unwrap();
                assert_eq!(
                    result,
                    Err(LOCAL_GUIDANCE.into()),
                    "{channel}: {reference:?}"
                );
                assert!(calls.lock().unwrap().is_empty(), "{channel}: {reference:?}");
            }
        }
        assert!(!core.worker_running().await);
    }

    #[tokio::test]
    async fn wrapped_declared_ids_block_before_worker_dispatch() {
        let (_temp, core, calls) = captured_core();
        for channel in [
            "extension:install",
            "extensions:install",
            "extensions:reinstall",
        ] {
            for reference in wrapped_private_ids() {
                let result = core.handle(channel, &[json!(&reference)]).await.unwrap();
                assert_eq!(
                    result,
                    Err(LOCAL_GUIDANCE.into()),
                    "{channel}: {reference:?}"
                );
                assert!(calls.lock().unwrap().is_empty(), "{channel}: {reference:?}");
                assert!(!core.worker_running().await, "{channel}: {reference:?}");
            }
        }
    }

    #[tokio::test]
    async fn non_equivalent_references_do_not_inherit_private_acquisition() {
        let (_temp, core, calls) = captured_core();
        for channel in [
            "extension:install",
            "extensions:install",
            "extensions:reinstall",
        ] {
            for reference in NON_EQUIVALENT_REFERENCES {
                let args = vec![json!(reference), json!({"unchanged":"fallback"})];
                assert_eq!(
                    core.handle(channel, &args).await.unwrap().unwrap(),
                    json!({"worker":"captured"}),
                    "{channel}: {reference:?}"
                );
                assert_eq!(
                    calls.lock().unwrap().last().unwrap(),
                    &(channel.to_owned(), args)
                );
            }
        }
        // The existing worker parser remains responsible for rejecting invalid
        // references; capture proves ordinary fallback keeps the original args.
        assert_eq!(
            calls.lock().unwrap().len(),
            NON_EQUIVALENT_REFERENCES.len() * 3
        );
        assert!(!core.worker_running().await);
    }

    #[tokio::test]
    async fn public_repository_install_still_dispatches_to_worker() {
        let (_temp, core, calls) = captured_core();
        for channel in [
            "extension:install",
            "extensions:install",
            "extensions:reinstall",
        ] {
            // Public catalog repository, unrelated public repository, and legacy ID.
            for reference in [
                "https://github.com/almatkai/amphetamine",
                "https://www.github.com/Example/Public-Extension.git/",
                " \t\u{feff}raycast.unrelated\u{00a0}\r\n",
            ] {
                let args = vec![json!(reference)];
                assert_eq!(
                    core.handle(channel, &args).await.unwrap().unwrap(),
                    json!({"worker":"captured"})
                );
                assert_eq!(
                    calls.lock().unwrap().last().unwrap(),
                    &(channel.to_owned(), args)
                );
            }
        }
        assert_eq!(calls.lock().unwrap().len(), 9);
        assert!(!core.worker_running().await);
    }
}
