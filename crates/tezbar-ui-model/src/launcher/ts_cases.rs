//! Each entry identifies one original TS test, including each `it.each` expansion.
use super::*;
fn strarg(v: &Value, i: usize) -> &str {
    v[i].as_str().unwrap_or("")
}
fn rows(v: &Value) -> Vec<Arc<Row>> {
    serde_json::from_value::<Vec<Row>>(v.clone())
        .unwrap()
        .into_iter()
        .map(Arc::new)
        .collect()
}
fn row_json(rows: impl IntoIterator<Item = impl AsRef<Row>>) -> Value {
    serde_json::to_value(
        rows.into_iter()
            .map(|r| {
                let mut v = serde_json::to_value(r.as_ref()).unwrap();
                let m = v.as_object_mut().unwrap();
                m.remove("ephemeral");
                if m["action"]["payload"].is_null() {
                    m["action"].as_object_mut().unwrap().remove("payload");
                }
                v
            })
            .collect::<Vec<_>>(),
    )
    .unwrap()
}
fn run(c: &Value) -> Value {
    let a = &c["args"];
    let s = |i| strarg(a, i);
    match c["function"].as_str().unwrap() {
        "commandBarInputMode" => {
            let m = input_mode(s(0), a[1].as_bool().unwrap());
            let mut v = json!({"parsedSearchQuery":m.parsed_search_query,"isDeepSearchMode":m.is_deep_search_mode,"isAiMode":m.mode == Mode::Ai,"aiTask":m.ai_task,"slashQuery":m.slash_query,"isSlashInput":m.is_slash_input,"isApplicationInput":m.is_application_input,"isCompletionInput":m.is_completion_input});
            if let Some(cwd) = m.ai_working_directory {
                v["aiWorkingDirectory"] = json!(cwd);
            }
            v
        }
        "aiChatBootForInput" => {
            let mut mode = input_mode(" ", false);
            mode.ai_task = a[0]["aiTask"].as_str().unwrap_or("").into();
            mode.ai_working_directory = a[0]["aiWorkingDirectory"].as_str().map(String::from);
            serde_json::to_value(ai_boot(&mode, a[1].as_str().map(String::from))).unwrap()
        }
        "sanitizeLauncherArgument" | "validateLauncherArgument" => {
            let v = if a[1].is_object() {
                Some(IntegerValidation {
                    min: a[1]["min"].as_u64().unwrap(),
                    max: a[1]["max"].as_u64().unwrap(),
                    max_length: a[1]["maxLength"].as_u64().unwrap() as usize,
                    message: a[1]["message"].as_str().unwrap().into(),
                })
            } else {
                None
            };
            if c["function"] == "sanitizeLauncherArgument" {
                json!(sanitize_argument(s(0), v.as_ref()))
            } else {
                json!(validate_argument(s(0), v.as_ref()).err())
            }
        }
        "optimisticSearchResults" => row_json(optimistic(&rows(&a[0]), s(1)).into_iter().cloned()),
        "launcherQueryHistoryEntry" => json!(history_entry(s(0), a[1].as_bool().unwrap())),
        "addLauncherQueryHistoryEntry" => {
            let mut state = crate::ui_state::UiState::default();
            state.record_launcher_query(s(0));
            json!(state.launcher_query_history)
        }
        "parseLauncherQueryHistory" => {
            let mut state = crate::ui_state::UiState::default();
            let _ = state.import_key("tezbar:launcher-query-history:v1", s(0));
            json!(state.launcher_query_history)
        }
        "shouldRecallLastLauncherQuery" => {
            let v = &a[0];
            json!(should_recall(
                v["key"].as_str().unwrap(),
                v["value"].as_str().unwrap(),
                v["isDeepSearchMode"].as_bool().unwrap(),
                v["selectedResultIndex"].as_i64().unwrap() as isize,
                v["visibleResultCount"].as_u64().unwrap() as usize
            ))
        }
        "parseSearchQuery" => json!(parse_search_query(s(0))),
        "deepSearchInput" => json!(deep_search_input(s(0))),
        "deepSearchDraftInput" => json!(deep_search_draft_input(s(0))),
        "searchRequestInput" => json!(request_input(s(0))),
        "hasGoodMetadataMatch" => json!(good_metadata(s(0), &rows(&a[1]))),
        "buildDeepSearchRecommendation" => deep_recommendation(s(0), &rows(&a[1]))
            .map(|r| row_json([Arc::new(r)])[0].clone())
            .unwrap_or(Value::Null),
        "searchQueryVariants" => json!(text::variants(s(0))),
        "applicationNameMatchScore" => json!(text::app_match(s(0), s(1))),
        f => panic!("Unmapped TS call {f}"),
    }
}
pub fn case(index: usize) {
    let cases: Value = serde_json::from_str(include_str!("ts_cases.json")).unwrap();
    let case = &cases[index];
    for c in case["calls"].as_array().unwrap() {
        let expected = &c["expected"];
        let got = run(c);
        if c["function"] == "applicationNameMatchScore" {
            let (a, b) = (got.as_f64().unwrap(), expected.as_f64().unwrap());
            assert!(
                (a - b).abs() < 1e-9,
                "{} :: {} :: {}: {a} != {b}",
                case["ts_file"],
                case["ts_case"],
                c["function"]
            );
        } else {
            assert_eq!(
                numeric_normalize(&got),
                numeric_normalize(expected),
                "{} :: {} :: {} args {}",
                case["ts_file"],
                case["ts_case"],
                c["function"],
                c["args"]
            );
        }
    }
}
#[test]
fn ts_000_gives_ai_mode_priority_over_slash_path_completion_after_double_space() {
    case(0);
}
#[test]
fn ts_001_keeps_ai_mode_active_and_searches_chats_after_typing_beyond_the_double_space_switch() {
    case(1);
}
#[test]
fn ts_002_preserves_regular_ai_activation_when_two_spaces_are_trailing() {
    case(2);
}
#[test]
fn ts_003_does_not_treat_an_ordinary_internal_double_space_query_as_ai_mode() {
    case(3);
}
#[test]
fn ts_004_preserves_leading_space_ai_prompts_without_treating_them_as_directories() {
    case(4);
}
#[test]
fn ts_005_returns_null_boot_for_empty_ai_mode_prompt_so_newchat_can_be_opened() {
    case(5);
}
#[test]
fn ts_006_retains_numeric_input_filtering_and_range_validation() {
    case(6);
}
#[test]
fn ts_007_puts_the_matching_app_and_extension_surface_above_generic_files() {
    case(7);
}
#[test]
fn ts_008_matches_corrected_layouts_in_the_first_frame_without_unrelated_cached_results() {
    case(8);
}
#[test]
fn ts_009_preserves_literal_russian_matches_and_supports_the_reverse_layout() {
    case(9);
}
#[test]
fn ts_010_prefers_a_literal_match_over_an_equally_relevant_corrected_match() {
    case(10);
}
#[test]
fn ts_011_does_not_carry_an_unrelated_high_score_result_into_a_typed_query() {
    case(11);
}
#[test]
fn ts_012_records_a_submitted_launcher_query_without_surrounding_whitespace() {
    case(12);
}
#[test]
fn ts_013_preserves_ai_mode_when_normalizing_a_recalled_prompt() {
    case(13);
}
#[test]
fn ts_014_leaves_terminal_commands_to_terminal_history() {
    case(14);
}
#[test]
fn ts_015_keeps_only_the_latest_query() {
    case(15);
}
#[test]
fn ts_016_only_recalls_the_last_query_after_arrowup_reaches_the_top_search_result() {
    case(16);
}
#[test]
fn ts_017_leaves_arrowdown_for_result_navigation() {
    case(17);
}
#[test]
fn ts_018_never_recalls_a_launcher_query_from_deep_search_mode() {
    case(18);
}
#[test]
fn ts_019_safely_parses_persisted_history() {
    case(19);
}
#[test]
fn ts_020_treats_a_leading_bang_as_deep_search_and_removes_it_from_the_indexed_query() {
    case(20);
}
#[test]
fn ts_021_does_not_change_the_backend_query_for_trailing_whitespace() {
    case(21);
}
#[test]
fn ts_022_accepts_a_complete_metadata_match() {
    case(22);
}
#[test]
fn ts_023_recommends_deep_search_for_weak_partial_matches_and_ignores_synthetic_rows() {
    case(23);
}
#[test]
fn ts_024_does_not_treat_short_path_fragments_as_a_match_for_a_punctuated_unicode_query() {
    case(24);
}
#[test]
fn ts_025_still_accepts_a_substantial_reverse_prefix_metadata_match() {
    case(25);
}
#[test]
fn ts_026_builds_an_immediate_recommendation_above_weak_local_candidates() {
    case(26);
}
#[test]
fn ts_027_keeps_a_matching_app_ahead_of_deep_search_for() {
    case(27);
}
#[test]
fn ts_028_keeps_a_matching_app_ahead_of_deep_search_for() {
    case(28);
}
#[test]
fn ts_029_keeps_a_matching_app_ahead_of_deep_search_for_chrome() {
    case(29);
}
#[test]
fn ts_030_recognizes_the_real_visual_studio_code_name_for_vs_code() {
    case(30);
}
#[test]
fn ts_031_recognizes_the_real_visual_studio_code_name_for() {
    case(31);
}
#[test]
fn ts_032_recognizes_the_real_visual_studio_code_name_for_vs() {
    case(32);
}
#[test]
fn ts_033_recognizes_the_real_visual_studio_code_name_for_code() {
    case(33);
}
#[test]
fn ts_034_recognizes_the_real_visual_studio_code_name_for() {
    case(34);
}
#[test]
fn ts_035_does_not_recommend_deep_search_when_local_metadata_is_already_close() {
    case(35);
}
#[test]
fn ts_036_corrects_russian_keys_to_english_without_discarding_the_literal_query() {
    case(36);
}
#[test]
fn ts_037_also_corrects_english_keys_to_russian() {
    case(37);
}
#[test]
fn ts_038_retains_already_correct_words_in_mixed_layout_input() {
    case(38);
}
#[test]
fn ts_039_handles_russian_letter_keys_on_punctuation_positions() {
    case(39);
}
#[test]
fn ts_040_does_not_expand_empty_numeric_or_punctuation_only_queries() {
    case(40);
}
#[test]
fn ts_041_keeps_real_russian_and_english_queries_as_the_first_choice() {
    case(41);
}
#[test]
fn ts_042_recognizes_visual_studio_code_by_the_alias_vs_code() {
    case(42);
}
#[test]
fn ts_043_recognizes_visual_studio_code_by_the_alias_vscode() {
    case(43);
}
#[test]
fn ts_044_recognizes_visual_studio_code_by_the_alias() {
    case(44);
}
#[test]
fn ts_045_recognizes_google_chrome_by_the_alias() {
    case(45);
}
#[test]
fn ts_046_recognizes_telegram_by_the_alias() {
    case(46);
}
#[test]
fn ts_047_recognizes_discord_by_the_alias() {
    case(47);
}
#[test]
fn ts_048_recognizes_safari_by_the_alias() {
    case(48);
}
#[test]
fn ts_049_matches_complete_name_tokens_including_partial_typing_without_dropping_terms() {
    case(49);
}
#[test]
fn ts_050_does_not_inherit_object_keys_or_add_aliases_to_unknown_applications() {
    case(50);
}

fn numeric_normalize(v: &Value) -> Value {
    match v {
        Value::Number(n) => json!(n.as_f64().unwrap()),
        Value::Array(a) => Value::Array(a.iter().map(numeric_normalize).collect()),
        Value::Object(m) => Value::Object(
            m.iter()
                .map(|(k, v)| (k.clone(), numeric_normalize(v)))
                .collect(),
        ),
        v => v.clone(),
    }
}
