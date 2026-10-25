use latex_math_symbols_native::{
    fresh_token, items, search, symbols, MAX_QUERY_UNITS, RECORD_COUNT,
};
use std::collections::HashSet;
#[test]
fn catalogue_preserves_every_field_variant_escape_and_order() {
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("../data/commands.json")).unwrap();
    assert_eq!(serde_json::to_value(symbols().unwrap()).unwrap(), expected);
    let items = items().unwrap();
    assert_eq!(items.len(), RECORD_COUNT);
    for (i, item) in items.iter().enumerate() {
        assert_eq!(item.id, i);
        assert_eq!(item.title, symbols().unwrap()[i].command);
        assert_eq!(item.subtitle, symbols().unwrap()[i].descriptions.join("; "));
    }
    assert_eq!(
        symbols()
            .unwrap()
            .iter()
            .filter(|s| s.variants.is_some())
            .count(),
        3
    );
    assert!(symbols().unwrap().iter().any(|s| s.command.contains('\n')));
    assert!(items.iter().any(|s| s.title == "\\Delta"));
    assert!(items.iter().any(|s| s.title == "\\delta"));
}
#[test]
fn empty_query_bypasses_fuse_and_resource_limits_are_errors() {
    let hits = search("").unwrap();
    assert_eq!(
        hits.iter().map(|h| h.item.clone()).collect::<Vec<_>>(),
        items().unwrap()
    );
    assert!(hits.iter().all(|h| h.score.is_none()));
    assert!(search(&"a".repeat(MAX_QUERY_UNITS + 1)).is_err());
    assert!(search(&"😀".repeat(MAX_QUERY_UNITS / 2 + 1)).is_err());
    assert!(search(&"a".repeat(MAX_QUERY_UNITS)).is_ok());
    assert!(search(" ").unwrap().iter().all(|h| h.score.is_some()));
}
#[test]
fn token_generations_are_unique_across_threads() {
    let threads: Vec<_> = (0..8)
        .map(|_| {
            std::thread::spawn(|| (0..250).map(|_| fresh_token().unwrap()).collect::<Vec<_>>())
        })
        .collect();
    let tokens: HashSet<_> = threads
        .into_iter()
        .flat_map(|t| t.join().unwrap())
        .collect();
    assert_eq!(tokens.len(), 2000);
}
#[test]
fn search_is_case_insensitive_ranked_and_variants_are_data_only() {
    for q in [
        "delta",
        "sum",
        "matrix",
        "sqrt",
        "not equal",
        "3.1415926535",
    ] {
        let lower = search(q).unwrap();
        let upper = search(&q.to_uppercase()).unwrap();
        assert_eq!(
            serde_json::to_value(&lower).unwrap(),
            serde_json::to_value(&upper).unwrap()
        );
        assert!(lower.windows(2).all(|p| p[0].score <= p[1].score));
    }
    assert_eq!(search("fraction").unwrap()[0].item.title, "\\frac{}{}");
    assert!(!items()
        .unwrap()
        .iter()
        .any(|s| s.title == "\\sum\\limits_{}^{}"));
}
