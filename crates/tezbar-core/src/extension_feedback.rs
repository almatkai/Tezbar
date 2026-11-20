//! Host presentation of terminal extension results; no new extension RPC/grants.
use serde_json::Value;
use std::{
    future::Future,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Notice(String);
fn text(value: &Value) -> Option<&str> {
    value.as_str().map(str::trim).filter(|v| !v.is_empty())
}
fn notice(result: &Result<Value, String>) -> Option<Notice> {
    let value = match result {
        Err(error) => return (!error.trim().is_empty()).then(|| Notice(error.trim().into())),
        Ok(value) => value,
    };
    if value["ok"] == false {
        if let Some(message) = text(&value["message"]) {
            return Some(Notice(message.into()));
        }
    }
    if let Some(effect) = value["effects"].as_array().and_then(|effects| {
        effects
            .iter()
            .rev()
            .find(|e| matches!(e["kind"].as_str(), Some("toast" | "hud")))
    }) {
        if effect["style"] == "animated" {
            return None;
        }
        let status = match (value["ok"] == false, effect["style"].as_str()) {
            (true, _) | (_, Some("failure")) => "Failed",
            (_, Some("success")) => "Saved",
            _ => "Done",
        };
        let title = text(&effect["title"]);
        let detail = text(&effect["message"]);
        if title.is_some() || detail.is_some() {
            let mut parts = vec![status];
            if let Some(title) = title.filter(|v| !v.eq_ignore_ascii_case(status)) {
                parts.push(title);
            }
            if let Some(detail) = detail {
                parts.push(detail);
            }
            return Some(Notice(parts.join(" · ")));
        }
    }
    text(&value["message"]).map(|v| Notice(v.into()))
}

pub(crate) fn strip_native_feedback(result: &mut Result<Value, String>) {
    if let Ok(Value::Object(value)) = result {
        value.remove("nativeFeedback");
    }
}

#[derive(Default)]
pub(super) struct Feedback {
    last: Mutex<Option<(Notice, Instant)>>,
}
impl Feedback {
    pub(super) async fn present<F, Fut>(
        &self,
        channel: &str,
        hidden: bool,
        result: &mut Result<Value, String>,
        deliver: F,
    ) -> Result<(), String>
    where
        F: FnOnce(String) -> Fut,
        Fut: Future<Output = Result<(), String>>,
    {
        // Do not turn 500 ms polling, search, background work or resource RPCs into alerts.
        if !hidden || !matches!(channel, "extension:run-command" | "extension:invoke-action") {
            return Ok(());
        }
        if result
            .as_ref()
            .is_ok_and(|value| value["nativeFeedback"] == true)
        {
            return Ok(()); // This exact result has already been presented by the host.
        }
        let Some(notice) = notice(result) else {
            return Ok(());
        };
        // Serialize submissions. One fixed OS identity bounds history to one item;
        // repeated identical feedback within two seconds does not produce another banner.
        let mut last = self.last.lock().await;
        if last.as_ref().is_some_and(|(previous, at)| {
            previous == &notice && at.elapsed() < Duration::from_secs(2)
        }) {
            // Rate limiting another result is not evidence that this result was delivered.
            // Leave it unmarked so the caller can still present its own status.
            return Ok(());
        }
        // Bound OS content; leave the complete original result intact for the renderer.
        deliver(notice.0.chars().take(512).collect()).await?;
        *last = Some((notice, Instant::now()));
        if let Ok(Value::Object(value)) = result {
            value.insert("nativeFeedback".into(), Value::Bool(true));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn selector_only_uses_explicit_terminal_notices_and_preserves_failures() {
        for value in [
            json!({"ok":true,"message":""}),
            json!({"ok":true,"effects":[{"kind":"clipboard","value":"private clipboard text"}]}),
            json!({"ok":true,"message":"fallback","effects":[{"kind":"toast","style":"animated","title":"Working"}]}),
        ] {
            assert_eq!(notice(&Ok(value)), None);
        }
        for style in ["success", "animated"] {
            let value = json!({"ok":false,"message":"Write failed; outcome uncertain","effects":[{"kind":"toast","style":style,"title":"Copied"}]});
            assert_eq!(
                notice(&Ok(value)),
                Some(Notice("Write failed; outcome uncertain".into()))
            );
        }
        assert_eq!(
            notice(&Ok(
                json!({"ok":true,"message":"Projected","effects":[{"kind":"hud","title":"Copied"}]})
            )),
            Some(Notice("Done · Copied".into()))
        );
    }

    #[tokio::test]
    async fn hidden_user_operations_deliver_once_and_preserve_the_result() {
        let feedback = Feedback::default();
        let deliveries = AtomicUsize::new(0);
        let original = json!({"ok":true,"mode":"unchanged","message":"Copied","effects":[{"kind":"clipboard","value":"secret"}]});
        let mut first = Ok(original.clone());
        let mut second = Ok(original.clone());
        for (index, result) in [&mut first, &mut second].into_iter().enumerate() {
            let deliveries = &deliveries;
            feedback
                .present(
                    "extension:invoke-action",
                    true,
                    result,
                    |message| async move {
                        assert_eq!(message, "Copied");
                        deliveries.fetch_add(1, Ordering::SeqCst);
                        Ok(())
                    },
                )
                .await
                .unwrap();
            let value = result.as_ref().unwrap();
            assert_eq!(value["nativeFeedback"] == true, index == 0);
            let mut clean = value.clone();
            clean.as_object_mut().unwrap().remove("nativeFeedback");
            assert_eq!(clean, original);
        }
        assert_eq!(deliveries.load(Ordering::SeqCst), 1);
        let mut hidden_error = Err("No text content found".to_string());
        feedback
            .present(
                "extension:run-command",
                true,
                &mut hidden_error,
                |message| async move {
                    assert_eq!(message, "No text content found");
                    Ok(())
                },
            )
            .await
            .unwrap();
        assert_eq!(hidden_error, Err("No text content found".into()));
    }

    #[tokio::test]
    async fn equal_text_from_another_call_does_not_claim_failure_delivery() {
        let feedback = Feedback::default();
        let mut first = Ok(json!({"ok":true,"message":"Same text"}));
        feedback
            .present("extension:run-command", true, &mut first, |_| async {
                Ok(())
            })
            .await
            .unwrap();
        let original = json!({"ok":false,"message":"Same text","details":{"call":"B"}});
        let mut second = Ok(original.clone());
        feedback
            .present("extension:invoke-action", true, &mut second, |_| async {
                panic!("identical notice should be rate limited")
            })
            .await
            .unwrap();
        assert_eq!(first.unwrap()["nativeFeedback"], true);
        assert_eq!(second, Ok(original));
    }

    #[tokio::test]
    async fn visible_polling_search_and_already_delivered_results_never_notify() {
        let feedback = Feedback::default();
        for (channel, hidden, native) in [
            ("extension:run-command", false, false),
            ("extension:refresh-session", true, false),
            ("extension:search-text-changed", true, false),
            ("extension:load-more", true, false),
            ("host.notifications.show", true, false),
            ("extension:run-command", true, true),
        ] {
            let mut result = Ok(json!({"ok":true,"message":"Saved"}));
            if native {
                result.as_mut().unwrap()["nativeFeedback"] = json!(true);
            }
            feedback
                .present(channel, hidden, &mut result, |_| async {
                    panic!("unexpected OS delivery")
                })
                .await
                .unwrap();
        }
    }

    #[tokio::test]
    async fn delivery_failure_does_not_mark_or_cache_and_unicode_body_is_bounded() {
        let feedback = Feedback::default();
        let original = json!({"ok":false,"message":"雪".repeat(600)});
        let mut result = Ok(original.clone());
        assert!(feedback
            .present(
                "extension:run-command",
                true,
                &mut result,
                |message| async move {
                    assert_eq!(message.chars().count(), 512);
                    Err("Permission not authorized".into())
                }
            )
            .await
            .is_err());
        assert_eq!(result, Ok(original));
        feedback
            .present("extension:run-command", true, &mut result, |_| async {
                Ok(())
            })
            .await
            .unwrap();
        assert_eq!(result.unwrap()["nativeFeedback"], true);
    }

    #[test]
    fn child_delivery_claims_are_always_stripped_without_changing_results() {
        let mut result = Ok(
            json!({"ok":false,"message":"Write failed","details":{"code":42},"nativeFeedback":true}),
        );
        strip_native_feedback(&mut result);
        assert_eq!(
            result,
            Ok(json!({"ok":false,"message":"Write failed","details":{"code":42}}))
        );
        let mut error = Err("Write failed".into());
        strip_native_feedback(&mut error);
        assert_eq!(error, Err("Write failed".into()));
    }
}
