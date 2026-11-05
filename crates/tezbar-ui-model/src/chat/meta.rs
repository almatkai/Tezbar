//! Offline chat metadata rules ported from shared/chatMeta.ts.
use regex::Regex;
pub fn icon_kind(icon: Option<&str>) -> String {
    let text = icon.unwrap_or("chat").trim().to_lowercase();
    match text.as_str() {
        "⚡" => "snippets".into(),
        "code" => "snippets".into(),
        "coding" => "snippets".into(),
        "dev" => "snippets".into(),
        "mcp" => "snippets".into(),
        "script" => "snippets".into(),
        "snippets" => "snippets".into(),
        "🔑" => "key".into(),
        "key" => "key".into(),
        "api-key" => "key".into(),
        "apikey" => "key".into(),
        "token" => "key".into(),
        "auth" => "key".into(),
        "password" => "key".into(),
        "💻" => "cpu".into(),
        "cpu" => "cpu".into(),
        "hardware" => "cpu".into(),
        "system" => "cpu".into(),
        "mac" => "cpu".into(),
        "pc" => "cpu".into(),
        "⚽" => "trophy".into(),
        "🏀" => "trophy".into(),
        "🎾" => "trophy".into(),
        "sport" => "trophy".into(),
        "sports" => "trophy".into(),
        "football" => "trophy".into(),
        "soccer" => "trophy".into(),
        "messi" => "trophy".into(),
        "ronaldo" => "trophy".into(),
        "trophy" => "trophy".into(),
        "📝" => "edit".into(),
        "✍️" => "edit".into(),
        "edit" => "edit".into(),
        "pencil" => "edit".into(),
        "draft" => "edit".into(),
        "notes" => "edit".into(),
        "note" => "edit".into(),
        "writing" => "edit".into(),
        "🤖" => "ai".into(),
        "ai" => "ai".into(),
        "llm" => "ai".into(),
        "sparkles" => "ai".into(),
        "📋" => "clipboard".into(),
        "clipboard" => "clipboard".into(),
        "☀️" => "weather".into(),
        "weather" => "weather".into(),
        "sun" => "weather".into(),
        "🎵" => "audio".into(),
        "music" => "audio".into(),
        "audio" => "audio".into(),
        "💰" => "brew".into(),
        "finance" => "brew".into(),
        "crypto" => "brew".into(),
        "money" => "brew".into(),
        "🔍" => "search".into(),
        "search" => "search".into(),
        "web" => "search".into(),
        "⚙️" => "settings".into(),
        "settings" => "settings".into(),
        "config" => "settings".into(),
        "📁" => "finder".into(),
        "folder" => "finder".into(),
        "files" => "finder".into(),
        "💬" => "chat".into(),
        "chat" => "chat".into(),
        _ => {
            if text.is_empty() {
                "chat".into()
            } else {
                text
            }
        }
    }
}
pub fn is_emoji(s: Option<&str>) -> bool {
    Regex::new(r"\p{Extended_Pictographic}")
        .expect("unicode pattern")
        .is_match(s.unwrap_or(""))
}
fn matches(text: &str, p: &str) -> bool {
    Regex::new(&format!(
        r"(?i)(?:^|[^\p{{L}}\p{{N}}])(?:{p})(?:[^\p{{L}}\p{{N}}]|$)"
    ))
    .expect("static pattern")
    .is_match(text)
}
pub fn clean(prompt: &str) -> String {
    let mut s = prompt
        .lines()
        .find(|s| !s.trim().is_empty())
        .unwrap_or("")
        .trim()
        .to_owned();
    let prefix=Regex::new(r"(?i)^(can you please|could you please|please tell me|tell me|can you|could you|please|who is|who was|what is|what was|what are|where is|when is|when did|when was|how to|how do i|how does|why is|why does|explain|help me with|i need to|look up|show me|which|list|подскажи пожалуйста|расскажи пожалуйста|скажи пожалуйста|скажите пожалуйста|подскажи мне пожалуйста|расскажи мне пожалуйста|скажи мне пожалуйста|скажите мне пожалуйста|покажи мне пожалуйста|подскажи мне|расскажи мне|скажи мне|скажите мне|покажи мне|подскажи|расскажи|скажи|скажите|пожалуйста|кто такой|кто такая|что такое|что за|как сделать|как настроить|как вообще|как|почему|зачем|объясни|найди|когда вышел|когда выйдет|когда|какие|какой|какая|какое|покажи)[,\s]+").expect("prefix");
    loop {
        let next = prefix
            .replace(&s, "")
            .trim_start_matches(|c: char| c == ',' || c == ':' || c.is_whitespace())
            .to_owned();
        if next == s {
            break;
        }
        s = next;
    }
    s = s.trim_end_matches(['?', '!', '.', ':', ';']).trim().into();
    let tail=Regex::new(r"(?i)\s+(when it came out|when did it come out|when was it released|когда вышел|когда выйдет|как вообще генерить|как генерить|как сделать|как настроить|как использовать)$").unwrap();
    s = tail
        .replace(&s, "")
        .trim_end_matches(['?', '!', '.', ':', ';'])
        .trim()
        .into();
    if let Some(first) = s.chars().next() {
        s = format!("{}{}", first.to_uppercase(), &s[first.len_utf8()..]);
    }
    if s.chars().count() > 36 {
        let short = s.chars().take(36).collect::<String>();
        let stop = short
            .rfind(' ')
            .filter(|p| short[..*p].chars().count() > 20)
            .unwrap_or(short.len());
        format!("{}…", short[..stop].trim())
    } else {
        s
    }
}
pub fn heuristic(prompt: &str, answer: Option<&str>) -> (String, String) {
    let text = format!("{} {}", prompt, answer.unwrap_or("")).to_lowercase();
    let clean = clean(prompt);
    for (pattern, title, icon) in [
        ("messi|месси", "Lionel Messi", "trophy"),
        ("ronaldo|роналду|криштиану", "Cristiano Ronaldo", "trophy"),
    ] {
        if matches(&text, pattern) {
            return (title.into(), icon.into());
        }
    }
    if matches(
        &text,
        "cpu|processor|процессор|память|ram|htop|activity monitor|нагрузк[а-я]*",
    ) {
        return (
            if matches(&text, "compiler|build|compil[a-z]*|idle|rustc|clang|gcc") {
                "CPU Usage & Compiler Memory"
            } else {
                "CPU & Process Diagnostics"
            }
            .into(),
            "cpu".into(),
        );
    }
    for (pattern, default, icon) in [
        (
            "api key|apikey|api-key|токен|token|auth|секрет|secret",
            "API Key & Auth",
            "key",
        ),
        (
            "port|ports|порт|порты|listening|слушает",
            "Listening Ports",
            "cpu",
        ),
        ("mcp", "MCP Protocol", "snippets"),
    ] {
        if matches(&text, pattern) {
            let title = if clean.is_empty() {
                default.into()
            } else if icon == "snippets" {
                Regex::new(r"(?i)\bversion\b")
                    .unwrap()
                    .replace_all(&clean, "Version")
                    .into_owned()
            } else {
                clean
            };
            return (title, icon.into());
        }
    }
    let mut icon = "chat";
    for(pattern,kind)in [
("football|soccer|fifa|champions league|футбол","trophy"),
("nba|basketball|баскетбол","sports"),
("tennis|теннис","sports"),
("rust|python|typescript|javascript|golang|c\\+\\+|react|vue|node|bun|docker|k8s|git|github|api|backend|frontend|mcp|кодинг|баг|ошибк[а-я]*","snippets"),
("cpu|mac|gpu|ram|terminal|bash|zsh|shell|linux|windows|диск|ssd|памят[а-я]*|процесс[а-я]*","cpu"),
("weather|rain|snow|temperature|погода|дождь|снег|температур[а-я]*","weather"),
("music|song|album|spotify|artist|песн[а-я]*|музык[а-я]*|трек[а-я]*","audio"),
("crypto|bitcoin|btc|eth|finance|stock|dollar|валют[а-я]*|деньг[а-я]*|биткоин|курс","brew"),
("ai|gpt|llm|claude|gemini|deepseek|ollama|нейросет[а-я]*|модел[а-я]*","ai"),
("write|essay|summary|draft|article|письм[а-я]*|стать[а-я]*|сочинени[а-я]*|пересказ","edit"),
("translate|english|russian|spanish|перевод[а-я]*|язык[а-я]*","finder"),
("movie|film|cinema|фильм[а-я]*|кино|сериал[а-я]*","picture"),
("recipe|food|cooking|coffee|еда|рецепт[а-я]*|кофе","coffee"),
("search|find|where|поиск|найди","search"),
]{if matches(&text,pattern){icon=kind;break;}}
    (
        if clean.is_empty() {
            "New chat".into()
        } else {
            clean
        },
        icon.into(),
    )
}
