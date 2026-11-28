//! Opaque saved-secret addresses are created and resolved only by their manifest owner.
use super::*;

#[derive(Clone, PartialEq, Eq)]
pub struct SecretSlot {
    revision: u64,
    extension: String,
    owner: String,
    provider: bool,
    key: String,
}
pub struct SecretDescriptor {
    pub label: String,
    pub scope: String,
    pub slot: SecretSlot,
}
impl Contributions {
    pub fn secret_slots(&self, extension: &str) -> Vec<SecretDescriptor> {
        let rows = self.raw["extensions"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|row| row["id"] == extension)
            .collect::<Vec<_>>();
        if rows.len() != 1 {
            return vec![];
        }
        let record = rows[0];
        let mut slots = Vec::new();
        for (collection, provider) in [("connections", false), ("providers", true)] {
            let owners = record[collection]
                .as_array()
                .into_iter()
                .flatten()
                .collect::<Vec<_>>();
            for owner in &owners {
                let Some(id) = owner["id"].as_str().filter(|id| !id.is_empty()) else {
                    continue;
                };
                if owners.iter().filter(|row| row["id"] == id).count() != 1 {
                    continue;
                }
                let title = owner[if provider { "title" } else { "name" }]
                    .as_str()
                    .unwrap_or(if provider { "Provider" } else { "Connection" });
                let mut add = |key: &str, label: &str, field: String| {
                    slots.push(SecretDescriptor {
                        label: label.into(),
                        scope: format!("{title} · {field}"),
                        slot: SecretSlot {
                            revision: self.raw_revision,
                            extension: extension.into(),
                            owner: id.into(),
                            provider,
                            key: key.into(),
                        },
                    });
                };
                for section in if provider {
                    &["headers"][..]
                } else {
                    &["env", "headers"][..]
                } {
                    for (field, reference) in owner[*section].as_object().into_iter().flatten() {
                        if let Some(key) = reference["secret"].as_str() {
                            add(
                                key,
                                reference["label"].as_str().unwrap_or("Saved secret"),
                                format!("{section}: {field}"),
                            );
                        }
                    }
                }
                if !provider && owner["auth"]["type"] == "token" {
                    add(
                        owner["auth"]["secret"].as_str().unwrap_or("access-token"),
                        "Access token",
                        "Sign-in".into(),
                    );
                }
            }
        }
        slots
    }
    pub fn set_saved_secret(
        &mut self,
        slot: &SecretSlot,
        value: String,
    ) -> Result<Effect, &'static str> {
        if value.is_empty() || value.len() > 128 * 1024 {
            return Err("Enter a secret under 128 KiB");
        }
        if slot.revision != self.raw_revision
            || !self
                .secret_slots(&slot.extension)
                .iter()
                .any(|d| d.slot == *slot)
        {
            return Err("The connection changed. Open its saved secrets again.");
        }
        let scope = if slot.provider {
            json!({"providerId":slot.owner})
        } else {
            json!(slot.owner)
        };
        Ok(self.request(
            format!("secret:{}:{}:{}", slot.extension, slot.owner, slot.key),
            Action::SetSecret,
            vec![json!(slot.extension), json!(slot.key), json!(value), scope],
        ))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn same_label_slots_keep_exact_connection_and_provider_namespaces() {
        let mut c = Contributions::default();
        let e = c.load();
        let raw = json!({"extensions":[{"id":"e","connections":[{"id":"a","name":"One","env":{"TOKEN":{"secret":"a-ref","label":"Key"}}},{"id":"b","name":"Two","headers":{"Auth":{"secret":"b-ref","label":"Key"}}}],"providers":[{"id":"custom:p","title":"Provider","headers":{"Auth":{"secret":"p-ref","label":"Key"}}}]}]});
        c.reply(e.token, Ok(raw.clone()));
        let slots = c.secret_slots("e");
        assert_eq!(slots.len(), 3);
        for (i, (key, scope)) in [
            ("a-ref", json!("a")),
            ("b-ref", json!("b")),
            ("p-ref", json!({"providerId":"custom:p"})),
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(slots[i].label, "Key");
            assert!(!slots[i].scope.contains("-ref"));
            let effect = c
                .set_saved_secret(&slots[i].slot, "fixture-value".into())
                .unwrap();
            assert_eq!(
                effect.args,
                vec![json!("e"), json!(key), json!("fixture-value"), scope]
            );
        }
        let e = c.load();
        c.reply(e.token, Ok(raw));
        assert!(c.set_saved_secret(&slots[0].slot, "late".into()).is_err());
        assert!(!c.display.to_string().contains("-ref"));
    }
    #[test]
    fn agent_and_provider_edits_send_only_the_changed_field_with_saved_refs() {
        let mut c = Contributions::default();
        let e = c.load();
        c.reply(e.token, Ok(json!({"extensions":[{"id":"e","agents":[{"id":"a","instructions":"old"}],"providers":[{"id":"custom:p","baseURL":"https://old.test","headers":{"X-Key":{"secret":"opaque","label":"Key"}}}]}]})));
        let effect = c
            .update("e", "agents", json!([{"id":"a","instructions":"new"}]))
            .unwrap();
        assert_eq!(
            effect.args[1],
            json!({"agents":[{"id":"a","instructions":"new"}]})
        );
        assert!(c.update("e", "profiles", json!([])).is_err());
        let effect = c
            .edit_row("e", "providers", 0, "baseURL", json!("https://new.test"))
            .unwrap();
        assert_eq!(
            effect.args[1]["providers"][0]["headers"]["X-Key"]["secret"],
            "opaque"
        );
        assert_eq!(effect.args[1].as_object().unwrap().len(), 1);
    }
}
