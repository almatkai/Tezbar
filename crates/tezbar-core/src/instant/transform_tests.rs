use super::super::qr;
use super::*;
use base64::Engine;
#[test]
fn transformers_01_formats_valid_json_with_2_space_indent_by_default() {
    assert_eq!(
        json_format(r#"{"a":1,"b":[2,3]}"#, true).unwrap(),
        r#"{
  "a": 1,
  "b": [
    2,
    3
  ]
}"#
    );
}
#[test]
fn transformers_02_minifies_valid_json() {
    assert_eq!(
        json_format(
            r#"{
  "a": 1,
  "b": [2, 3]
}"#,
            false
        )
        .unwrap(),
        r#"{"a":1,"b":[2,3]}"#
    );
}
#[test]
fn transformers_03_round_trips_format_minify() {
    let s = r#"{"x":[1,{"y":"z"}]}"#;
    assert_eq!(
        json_format(&json_format(s, true).unwrap(), false).unwrap(),
        s
    );
}
#[test]
fn transformers_04_reports_invalid_json_with_the_parse_error() {
    assert!(json_format("{not json", true)
        .unwrap_err()
        .contains("Invalid JSON"));
}
#[test]
fn transformers_05_rejects_empty_input_without_calling_json_parse() {
    assert!(json_format("   ", true).is_err());
    assert!(json_format("", false).is_err());
}
#[test]
fn transformers_06_encodes_ascii() {
    assert_eq!(base64_encode("hello world").unwrap(), "aGVsbG8gd29ybGQ=");
}
#[test]
fn transformers_07_encodes_utf_8_emoji_cjk_without_data_loss() {
    let s = "こんにちは 🌍";
    assert_eq!(base64_decode(&base64_encode(s).unwrap()).unwrap(), s);
}
#[test]
fn transformers_08_decodes_standard_base64() {
    assert_eq!(base64_decode("aGVsbG8gd29ybGQ=").unwrap(), "hello world");
}
#[test]
fn transformers_09_accepts_url_safe_base64_and_missing_padding() {
    assert_eq!(base64_decode("Pz8").unwrap(), "??");
    let s = base64_encode("?>~?")
        .unwrap()
        .replace('+', "-")
        .replace('/', "_")
        .trim_end_matches('=')
        .to_string();
    assert_eq!(base64_decode(&s).unwrap(), "?>~?");
}
#[test]
fn transformers_10_rejects_malformed_base64() {
    assert!(base64_decode("!!! not base64 !!!").is_err());
}
#[test]
fn transformers_11_encodes_reserved_characters() {
    assert_eq!(url_encode("a+b?c=d&e=f").unwrap(), "a%2Bb%3Fc%3Dd%26e%3Df");
}
#[test]
fn transformers_12_encodes_unicode() {
    assert_eq!(
        url_encode("こんにちは").unwrap(),
        "%E3%81%93%E3%82%93%E3%81%AB%E3%81%A1%E3%81%AF"
    );
}
#[test]
fn transformers_13_round_trips() {
    let s = "https://example.com/path?q=hello world&lang=日本語";
    assert_eq!(url_decode(&url_encode(s).unwrap(), true).unwrap(), s);
}
#[test]
fn transformers_14_decodes_plus_as_space_for_form_encoded_strings() {
    assert_eq!(url_decode("hello+world", true).unwrap(), "hello world");
}
#[test]
fn transformers_15_rejects_malformed_percent_encoding() {
    assert!(url_decode("%e3%81", true).is_err());
    assert!(url_decode("%zz", true).is_err());
}
#[test]
fn transformers_16_computes_md5() {
    for (s, h) in [
        ("hello", "5d41402abc4b2a76b9719d911017c592"),
        ("", "d41d8cd98f00b204e9800998ecf8427e"),
        (
            "The quick brown fox jumps over the lazy dog",
            "9e107d9d372bb6826bd81d3542a419d6",
        ),
    ] {
        assert_eq!(md5_hex(s.as_bytes()), h);
    }
}
#[test]
fn transformers_17_computes_sha256_via_webcrypto() {
    assert_eq!(
        hash_text("hello", HashAlgorithm::Sha256).unwrap(),
        "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
    );
}
#[test]
fn transformers_18_computes_sha512_via_webcrypto() {
    assert_eq!(hash_text("hello",HashAlgorithm::Sha512).unwrap(),"9b71d224bd62f3785d96d46ad3ea3d73319bfbc2890caadae2dff72519673ca72323c3d99ba5c11d7c7acc6e14b8c5da0c4663475c2e5c3adef46f73bcdec043");
}
#[test]
fn transformers_19_computes_md5_through_the_uniform_async_hash_api() {
    assert_eq!(
        hash_text("hello", HashAlgorithm::Md5).unwrap(),
        "5d41402abc4b2a76b9719d911017c592"
    );
}
#[test]
fn transformers_20_handles_unicode_input() {
    assert_eq!(
        hash_text("こんにちは", HashAlgorithm::Sha256).unwrap(),
        "125aeadf27b0459b8760c13a3d80912dfa8a81a68261906f60d87f4a0268646c"
    );
}
#[test]
fn transformers_21_rejects_empty_input() {
    assert!(hash_text("", HashAlgorithm::Sha256).is_err());
}
#[test]
fn transformers_22_counts_words_characters_sentences_paragraphs_lines() {
    let c = count_text("Hello world. How are you?\n\nI am fine.");
    assert_eq!((c.words, c.sentences, c.paragraphs, c.lines), (8, 3, 2, 3));
}
#[test]
fn transformers_23_handles_contractions_as_a_single_word() {
    assert_eq!(count_text("don't stop believing").words, 3);
}
#[test]
fn transformers_24_counts_unicode_words() {
    assert_eq!(count_text("こんにちは 世界").words, 2);
}
#[test]
fn transformers_25_counts_characters_as_code_points_not_utf_16_units() {
    assert_eq!(count_text("🌍").characters, 1);
}
#[test]
fn transformers_26_formats_the_report() {
    let s = word_count("one two three").unwrap();
    assert!(s.contains("Words: 3"));
    assert!(s.contains("Characters: 13"));
}
#[test]
fn transformers_27_rejects_empty_input() {
    assert!(word_count("   ").is_err());
}
#[test]
fn transformers_28_splits_on_spaces() {
    assert_eq!(split_words("hello world"), vec!["hello", "world"]);
}
#[test]
fn transformers_29_splits_camelcase_and_pascalcase() {
    for s in ["helloWorld", "HelloWorld"] {
        assert_eq!(split_words(s).len(), 2);
    }
    assert_eq!(split_words("helloWorld"), vec!["hello", "World"]);
}
#[test]
fn transformers_30_splits_snake_case_and_kebab_case() {
    for s in ["hello_world", "hello-world"] {
        assert_eq!(split_words(s), vec!["hello", "world"]);
    }
}
#[test]
fn transformers_31_handles_acronym_word_boundaries() {
    assert_eq!(
        split_words("HTTPServerError"),
        vec!["HTTP", "Server", "Error"]
    );
}
#[test]
fn transformers_32_converts_to_camelcase() {
    assert_eq!(
        convert_case("hello world example", CaseStyle::Camel).unwrap(),
        "helloWorldExample"
    );
    assert_eq!(
        convert_case("hello_world", CaseStyle::Camel).unwrap(),
        "helloWorld"
    );
}
#[test]
fn transformers_33_converts_to_snake_case() {
    assert_eq!(
        convert_case("helloWorld", CaseStyle::Snake).unwrap(),
        "hello_world"
    );
}
#[test]
fn transformers_34_converts_to_kebab_case() {
    assert_eq!(
        convert_case("HelloWorld", CaseStyle::Kebab).unwrap(),
        "hello-world"
    );
}
#[test]
fn transformers_35_converts_to_constant_case() {
    assert_eq!(
        convert_case("helloWorld", CaseStyle::Constant).unwrap(),
        "HELLO_WORLD"
    );
}
#[test]
fn transformers_36_converts_to_title_case() {
    assert_eq!(
        convert_case("hello world", CaseStyle::Title).unwrap(),
        "Hello World"
    );
}
#[test]
fn transformers_37_converts_to_lower_and_upper() {
    assert_eq!(
        convert_case("Hello WORLD", CaseStyle::Lower).unwrap(),
        "hello world"
    );
    assert_eq!(
        convert_case("Hello world", CaseStyle::Upper).unwrap(),
        "HELLO WORLD"
    );
}
#[test]
fn transformers_38_round_trips_camel_snake() {
    assert_eq!(
        convert_case(
            &convert_case("parseHttpResponse", CaseStyle::Snake).unwrap(),
            CaseStyle::Camel
        )
        .unwrap(),
        "parseHttpResponse"
    );
}
#[test]
fn transformers_39_rejects_empty_input() {
    assert!(convert_case("  ", CaseStyle::Camel).is_err());
}
#[test]
fn transformers_40_parses_a_unix_ms_timestamp() {
    let s = parse_timestamp("1704067200000", 1704067200000).unwrap();
    for p in [
        "Unix (ms): 1704067200000",
        "ISO 8601: 2024-01-01T00:00:00.000Z",
        "Local: ",
        "Relative: ",
    ] {
        assert!(s.contains(p));
    }
}
#[test]
fn transformers_41_auto_detects_unix_seconds() {
    assert!(parse_timestamp("1704067200", 0)
        .unwrap()
        .contains("ISO 8601: 2024-01-01T00:00:00.000Z"));
}
#[test]
fn transformers_42_rejects_non_numeric_input() {
    for s in ["tomorrow", "2024-01-01"] {
        assert!(parse_timestamp(s, 0).is_err());
    }
}
#[test]
fn transformers_43_rejects_empty_input() {
    assert!(parse_timestamp("   ", 0).is_err());
}
#[test]
fn transformers_44_returns_a_base64_svg_data_url() {
    let s = qr::svg_data_url("https://example.com").unwrap();
    assert!(s.starts_with("data:image/svg+xml;base64,"));
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(s.split_once(',').unwrap().1)
        .unwrap();
    let svg = String::from_utf8(bytes).unwrap();
    for p in ["<svg", "</svg>", "viewBox"] {
        assert!(svg.contains(p));
    }
}
#[test]
fn transformers_45_is_deterministic_same_input_same_data_url() {
    assert_eq!(qr::svg_data_url("hello"), qr::svg_data_url("hello"));
}
#[test]
fn transformers_46_produces_different_output_for_different_inputs() {
    assert_ne!(qr::svg_data_url("hello"), qr::svg_data_url("world"));
}
#[test]
fn transformers_47_rejects_empty_input() {
    assert!(qr::svg_data_url("").is_err());
}
#[test]
fn transformers_48_encodes_longer_payloads_v2_path() {
    assert!(
        qr::svg_data_url("The quick brown fox jumps over the lazy dog — and keeps going.").is_ok()
    );
}
#[test]
fn transformers_49_rejects_payloads_too_large_for_version_10() {
    assert!(qr::svg_data_url(&"x".repeat(500))
        .unwrap_err()
        .contains("too long"));
}
#[test]
fn transformers_50_round_trips_short_inputs() {
    for s in ["A", "hello", "https://example.com"] {
        assert_eq!(decode_qr(&qr::svg_data_url(s).unwrap()), s);
    }
}
#[test]
fn transformers_51_round_trips_longer_inputs_forces_multi_block_rs_alignment_patterns() {
    for n in [70, 100, 150, 200, 213] {
        let s = "x".repeat(n);
        assert_eq!(decode_qr(&qr::svg_data_url(&s).unwrap()), s);
    }
}
#[test]
fn transformers_52_round_trips_unicode_payloads() {
    let s = "こんにちは 🌍";
    assert_eq!(decode_qr(&qr::svg_data_url(s).unwrap()), s);
}
#[test]
fn clipboard_01_formats_json() {
    assert!(apply_clipboard_action(ClipboardAction::Json, r#"{"a":1}"#)
        .unwrap()
        .contains(
            r#"
  "a": 1
"#
        ));
}
#[test]
fn clipboard_02_encodes_then_decodes_base64() {
    assert_eq!(
        apply_clipboard_action(
            ClipboardAction::Base64,
            &apply_clipboard_action(ClipboardAction::Base64, "hello").unwrap()
        )
        .unwrap(),
        "hello"
    );
}
#[test]
fn clipboard_03_url_encodes_and_decodes() {
    let s = apply_clipboard_action(ClipboardAction::Encode, "a b&c").unwrap();
    assert_eq!(s, "a%20b%26c");
    assert_eq!(
        apply_clipboard_action(ClipboardAction::Encode, &s).unwrap(),
        "a b&c"
    );
}
#[test]
fn clipboard_04_counts_words_chars_lines() {
    let s = apply_clipboard_action(ClipboardAction::Count, "one two\nthree").unwrap();
    for p in ["Characters : 13", "Words      : 3", "Lines      : 2"] {
        assert!(s.contains(p));
    }
}
#[test]
fn clipboard_05_flips_case() {
    assert_eq!(
        apply_clipboard_action(ClipboardAction::Case, "hello").unwrap(),
        "HELLO"
    );
}
#[test]
fn clipboard_06_hashes_with_sha256() {
    let s = apply_clipboard_action(ClipboardAction::Hash, "test").unwrap();
    assert_eq!(s.len(), 64);
    assert!(s.bytes().all(|b| b.is_ascii_hexdigit()));
}
#[test]
fn clipboard_07_generates_qr_data_url() {
    assert!(
        apply_clipboard_action(ClipboardAction::Qr, "https://example.com")
            .unwrap()
            .starts_with("data:image/svg+xml")
    );
}
#[test]
fn clipboard_08_generates_qr_for_whitespace_only_input_renderer_handles_trimming() {
    assert!(apply_clipboard_action(ClipboardAction::Qr, "   ")
        .unwrap()
        .starts_with("data:image/svg"));
}
// Independent byte-mode QR decode, matching the TS structural regression. Reads the actual
// format mask, skips function modules and undoes level-M Reed-Solomon block interleaving.
fn decode_qr(url: &str) -> String {
    let svg = String::from_utf8(
        base64::engine::general_purpose::STANDARD
            .decode(url.split_once(',').unwrap().1)
            .unwrap(),
    )
    .unwrap();
    let n = Regex::new(r#"viewBox="0 0 (\d+)"#)
        .unwrap()
        .captures(&svg)
        .unwrap()[1]
        .parse::<usize>()
        .unwrap()
        - 8;
    let mut grid = vec![vec![0usize; n]; n];
    for c in Regex::new(r"M(\d+),(\d+)h1v1h-1z")
        .unwrap()
        .captures_iter(&svg)
    {
        let x = c[1].parse::<usize>().unwrap() - 4;
        let y = c[2].parse::<usize>().unwrap() - 4;
        grid[y][x] = 1;
    }
    let version = (n - 17) / 4;
    let alignment: [&[usize]; 10] = [
        &[],
        &[6, 18],
        &[6, 22],
        &[6, 26],
        &[6, 30],
        &[6, 34],
        &[6, 22, 38],
        &[6, 24, 42],
        &[6, 26, 46],
        &[6, 28, 50],
    ];
    let is_fn = |r: usize, c: usize| {
        if r <= 8 && c <= 8
            || r <= 8 && c >= n - 8
            || r >= n - 7 && c <= 8
            || r == 6
            || c == 6
            || c == 8 && r >= n - 8
            || r == 8 && c >= n - 7
        {
            return true;
        }
        if n >= 45
            && (r <= 5 && (n - 11..=n - 9).contains(&c) || c <= 5 && (n - 11..=n - 9).contains(&r))
        {
            return true;
        }
        for &ar in alignment[version - 1] {
            for &ac in alignment[version - 1] {
                if (ar == 6 && (ac == 6 || ac == n - 7)) || (ar == n - 7 && ac == 6) {
                    continue;
                }
                if r.abs_diff(ar) <= 2 && c.abs_diff(ac) <= 2 {
                    return true;
                }
            }
        }
        false
    };
    let mut format = 0;
    for i in 0..15 {
        let r = if i < 6 {
            i
        } else if i < 8 {
            i + 1
        } else {
            n - 15 + i
        };
        format |= grid[r][8] << i;
    }
    let mask = ((format ^ 0x5412) >> 10) & 7;
    let mut bits = vec![];
    let mut up = true;
    let mut col = n - 1;
    loop {
        if col == 6 {
            col -= 1;
        }
        for j in 0..n {
            let r = if up { n - 1 - j } else { j };
            for c in [col, col - 1] {
                if is_fn(r, c) {
                    continue;
                }
                let masked = match mask {
                    0 => (r + c) % 2 == 0,
                    1 => r % 2 == 0,
                    2 => c % 3 == 0,
                    3 => (r + c) % 3 == 0,
                    4 => (r / 2 + c / 3) % 2 == 0,
                    5 => (r * c) % 2 + (r * c) % 3 == 0,
                    6 => ((r * c) % 2 + (r * c) % 3) % 2 == 0,
                    _ => ((r + c) % 2 + (r * c) % 3) % 2 == 0,
                };
                bits.push(grid[r][c] ^ usize::from(masked));
            }
        }
        up = !up;
        if col < 3 {
            break;
        }
        col -= 2;
    }
    let lengths: [&[usize]; 10] = [
        &[16],
        &[28],
        &[44],
        &[32, 32],
        &[43, 43],
        &[27, 27, 27, 27],
        &[20, 20, 20, 20],
        &[38, 38, 39, 39],
        &[36, 36, 36, 37, 37],
        &[43, 43, 43, 43, 44],
    ];
    let lengths = lengths[version - 1];
    let mut blocks = vec![vec![]; lengths.len()];
    let mut word = 0;
    for i in 0..*lengths.iter().max().unwrap() {
        for b in 0..blocks.len() {
            if i < lengths[b] {
                blocks[b].extend_from_slice(&bits[word * 8..word * 8 + 8]);
                word += 1;
            }
        }
    }
    let payload: Vec<_> = blocks.into_iter().flatten().collect();
    let read =
        |start: usize, len: usize| payload[start..start + len].iter().fold(0, |v, b| v * 2 + b);
    assert_eq!(read(0, 4), 4);
    let count_bits = if version < 10 { 8 } else { 16 };
    let count = read(4, count_bits);
    String::from_utf8(
        (0..count)
            .map(|i| read(4 + count_bits + i * 8, 8) as u8)
            .collect(),
    )
    .unwrap()
}
#[test]
fn qr_png_raster_is_512_and_error_is_explicit() {
    let bytes = qr::png("https://example.com").unwrap();
    let img = image::load_from_memory(&bytes).unwrap();
    assert_eq!((img.width(), img.height()), (512, 512));
    assert!(qr::png("").is_err());
}

#[test]
fn opus_json_js_numbers_bom_and_depth() {
    assert_eq!(
        json_format(
            "\u{feff}{\"a\":10.0,\"b\":1e2,\"c\":-0.0,\"d\":12345678901234567890}",
            false
        )
        .unwrap(),
        r#"{"a":10,"b":100,"c":0,"d":12345678901234567000}"#
    );
    let deep = "[".repeat(150) + &"]".repeat(150);
    assert!(apply_clipboard_action(ClipboardAction::Json, &deep).is_err());
}
#[test]
fn opus_base64_atob_trailing_bits() {
    assert_eq!(base64_decode("QR==").unwrap(), "A");
}

#[test]
fn opus_json_duplicate_order_large_objects_scaling() {
    assert_eq!(
        json_format(r#"{"z":1,"2":2,"a":3,"z":4,"1":5}"#, false).unwrap(),
        r#"{"1":5,"2":2,"z":4,"a":3}"#
    );
    fn base36(mut n: usize) -> String {
        let mut s = vec![];
        loop {
            s.push(b"0123456789abcdefghijklmnopqrstuvwxyz"[n % 36] as char);
            n /= 36;
            if n == 0 {
                break;
            }
        }
        s.into_iter().rev().collect()
    }
    for n in [5_000, 20_000, 40_000, 100_000] {
        let s = format!(
            "{{{}}}",
            (0..n)
                .map(|i| format!("\"k{}\":1", base36(i)))
                .collect::<Vec<_>>()
                .join(",")
        );
        assert!(s.len() <= super::super::MAX_INPUT_BYTES);
        let start = std::time::Instant::now();
        let output = json_format(&s, false).unwrap();
        let elapsed = start.elapsed();
        assert_eq!(output, s); // no data loss, all keys survive, deterministic order
        eprintln!(
            "json_scaling keys={n} bytes={} debug_ms={:.3}",
            s.len(),
            elapsed.as_secs_f64() * 1000.
        );
    }
}
#[test]
fn opus_locale_structured_data_and_injected_formatter() {
    struct Locale;
    impl PresentationFormatter for Locale {
        fn local_timestamp(&self, ms: i64) -> String {
            format!("local({ms})")
        }
        fn relative(&self, r: &RelativeTime) -> String {
            format!("{}:{}", r.unit, r.value)
        }
        fn count(&self, n: usize) -> String {
            format!("#{n}")
        }
    }
    let t = parse_timestamp_data("1704067200", 1704672000000).unwrap();
    assert_eq!(t.ms, 1704067200000);
    assert_eq!(
        t.relative,
        RelativeTime {
            unit: "week",
            value: -1
        }
    );
    assert!(
        format_timestamp(&t, &Locale).contains("Local: local(1704067200000)\nRelative: week:-1")
    );
    assert_eq!(format_relative(0, 604800000), "last week");
    assert_eq!(format_relative(2592000000, 0), "next month");
    assert_eq!(format_relative(0, 31536000000), "last year");
    let c = clipboard_count(&"x".repeat(1234));
    assert!(format_clipboard_count(&c, &Locale).starts_with("Characters : #1234"));
    assert!(
        apply_clipboard_action(ClipboardAction::Count, &"x".repeat(1234))
            .unwrap()
            .starts_with("Characters : 1,234")
    );
}
