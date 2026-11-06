//! Fixed HTML formatting subset. Tokenization never constructs a DOM or executes markup.
use crate::markdown::{Block, Kind, Span, safe_image, safe_link};
use html5ever::tokenizer::{
    BufferQueue, TagKind, Token, TokenSink, TokenSinkResult, Tokenizer, TokenizerOpts,
};
use std::cell::RefCell;
struct Sink(RefCell<Vec<Token>>);
impl TokenSink for Sink {
    type Handle = ();
    fn process_token(&self, token: Token, _: u64) -> TokenSinkResult<()> {
        self.0.borrow_mut().push(token);
        TokenSinkResult::Continue
    }
}
#[derive(Default)]
pub(crate) struct Html {
    suppressed: Vec<String>,
    details: bool,
}
impl Html {
    pub fn fragment(
        &mut self,
        html: &str,
        blocks: &mut Vec<Block>,
        current: &mut Block,
        style: &mut Span,
    ) {
        let tokenizer = Tokenizer::new(Sink(RefCell::new(vec![])), TokenizerOpts::default());
        let input = BufferQueue::default();
        input.push_back(html.into());
        let _ = tokenizer.feed(&input);
        tokenizer.end();
        for token in tokenizer.sink.0.into_inner() {
            match token {
                Token::TagToken(tag) => {
                    let name = tag.name.as_ref();
                    let start = tag.kind == TagKind::StartTag;
                    if matches!(name, "script" | "style" | "iframe" | "object") {
                        if start {
                            self.suppressed.push(name.into());
                        } else if self.suppressed.last().is_some_and(|s| s == name) {
                            self.suppressed.pop();
                        }
                        continue;
                    }
                    if !self.suppressed.is_empty() {
                        continue;
                    }
                    let attr = |key: &str| {
                        tag.attrs
                            .iter()
                            .find(|a| a.name.local.as_ref() == key)
                            .map(|a| a.value.as_ref())
                    };
                    match name {
                        "strong" | "b" => style.strong = start,
                        "em" | "i" => style.emphasis = start,
                        "u" => style.underline = start,
                        "s" | "del" => style.strike = start,
                        "code" | "kbd" => style.code = start,
                        "a" => {
                            style.link = if start {
                                attr("href").and_then(safe_link)
                            } else {
                                None
                            }
                        }
                        "img" if start => {
                            if let Some(url) = attr("src").and_then(safe_image) {
                                current.images.push((url, attr("alt").unwrap_or("").into()));
                            }
                        }
                        "br" if start => current.spans.push(Span {
                            text: "\n".into(),
                            ..Default::default()
                        }),
                        "details" => {
                            if start {
                                flush(blocks, current);
                                self.details = true;
                                current.kind = Kind::Details;
                            } else {
                                flush(blocks, current);
                                self.details = false;
                            }
                        }
                        "summary" if !start => current.spans.push(Span {
                            text: "\n".into(),
                            ..Default::default()
                        }),
                        "p" | "div" | "li" | "blockquote" | "pre" | "h1" | "h2" | "h3" | "h4"
                        | "h5" | "h6" | "table" => {
                            if self.details {
                                if !start {
                                    current.spans.push(Span {
                                        text: "\n".into(),
                                        ..Default::default()
                                    });
                                }
                                continue;
                            }
                            flush(blocks, current);
                            if start {
                                current.kind = match name {
                                    "li" => Kind::List,
                                    "blockquote" => Kind::Quote,
                                    "pre" => Kind::Code(String::new()),
                                    "table" => Kind::Table,
                                    n if n.starts_with('h') => {
                                        Kind::Heading(n[1..].parse().unwrap_or(1))
                                    }
                                    _ => Kind::Paragraph,
                                };
                            }
                        }
                        "td" | "th" if !start => current.spans.push(Span {
                            text: "\t".into(),
                            ..Default::default()
                        }),
                        "tr" if !start => current.spans.push(Span {
                            text: "\n".into(),
                            ..Default::default()
                        }),
                        "hr" if start => {
                            flush(blocks, current);
                            blocks.push(Block {
                                kind: Kind::Rule,
                                spans: vec![],
                                images: vec![],
                                checked: None,
                            });
                        }
                        _ => {}
                    }
                }
                Token::CharacterTokens(text) if self.suppressed.is_empty() => {
                    crate::markdown::append_span(
                        &mut current.spans,
                        Span {
                            text: text.to_string(),
                            ..style.clone()
                        },
                    )
                }
                Token::NullCharacterToken if self.suppressed.is_empty() => {
                    current.spans.push(Span {
                        text: "�".into(),
                        ..style.clone()
                    })
                }
                _ => {}
            }
        }
    }
}
fn flush(blocks: &mut Vec<Block>, current: &mut Block) {
    if !current.spans.is_empty() || !current.images.is_empty() {
        blocks.push(current.clone());
    }
    *current = Block {
        kind: Kind::Paragraph,
        spans: vec![],
        images: vec![],
        checked: None,
    };
}
