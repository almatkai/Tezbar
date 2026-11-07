//! Inline first-run setup only; installed Extensions Settings belongs to M7.
use super::*;
use tezbar_ui_model::extensions::forms::{PreferenceField, Preferences};
#[derive(Default)]
pub(super) struct Setup {
    pub identity: Option<(String, String)>,
    pub schema: Vec<Value>,
    pub model: Preferences,
    pub values: BTreeMap<String, Value>,
    pub fields: BTreeMap<String, FieldEntry>,
    pub saving: bool,
}
impl RuntimeView {
    pub(super) fn prepare_preferences(
        &mut self,
        root: &Node,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = &self.session else {
            return;
        };
        let identity = (
            session.key.extension_id.clone(),
            session.key.command_name.clone(),
        );
        if self.preference.identity.as_ref() == Some(&identity) {
            return;
        }
        let schema = root
            .props
            .get("preferences")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut groups: BTreeMap<Option<String>, (Vec<PreferenceField>, BTreeMap<String, Value>)> =
            BTreeMap::new();
        for f in &schema {
            let Some(name) = f["name"].as_str() else {
                continue;
            };
            let command = f["commandName"].as_str().map(str::to_owned);
            let group = groups.entry(command.clone()).or_default();
            group.0.push(PreferenceField {
                name: name.into(),
                command,
                kind: f["type"].as_str().unwrap_or("textfield").into(),
                required: f["required"] == true,
                options: f["data"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|v| v["value"].as_str().map(str::to_owned))
                    .collect(),
            });
            group.1.insert(
                name.into(),
                root.props
                    .get("values")
                    .and_then(|v| v.get(name))
                    .cloned()
                    .or_else(|| f.get("default").cloned())
                    .unwrap_or_else(|| {
                        if f["type"] == "checkbox" {
                            json!(false)
                        } else {
                            json!("")
                        }
                    }),
            );
        }
        let mut model =
            Preferences::merge(groups.into_iter().map(|(c, (f, v))| (c, f, v)).collect());
        let mut values = BTreeMap::new();
        let mut fields = BTreeMap::new();
        for f in model.fields.clone() {
            let key = f.key();
            let value = root
                .props
                .get("values")
                .and_then(|v| v.get(&f.name))
                .cloned()
                .or_else(|| {
                    schema
                        .iter()
                        .find(|v| v["name"] == f.name)
                        .and_then(|v| v.get("default").cloned())
                })
                .unwrap_or_else(|| {
                    if f.kind == "checkbox" {
                        json!(false)
                    } else {
                        json!("")
                    }
                });
            let _ = model.edit(&key, value.clone());
            values.insert(key.clone(), value.clone());
            if matches!(f.kind.as_str(), "checkbox" | "dropdown") {
                continue;
            }
            let input = cx.new(TextInput::new);
            let field = key.clone();
            let sub = cx.subscribe(&input, move |v, _, e: &Committed, cx| {
                let _ = v.preference.model.edit(&field, json!(e.0));
                v.preference.values.insert(field.clone(), json!(e.0));
                cx.notify();
            });
            input.update(cx, |v, cx| {
                v.configure(FieldOptions {
                    name: f.name.clone().into(),
                    password: f.kind == "password",
                    ..Default::default()
                });
                v.set_text(value.as_str().unwrap_or(""), w, cx);
            });
            fields.insert(
                key,
                FieldEntry {
                    input,
                    _subscription: sub,
                },
            );
        }
        self.preference = Setup {
            identity: Some(identity),
            schema,
            model,
            values,
            fields,
            saving: false,
        };
    }
    pub(super) fn edit_preference(&mut self, key: String, value: Value, cx: &mut Context<Self>) {
        if self.preference.saving {
            return;
        }
        if self.preference.model.edit(&key, value.clone()).is_ok() {
            self.preference.values.insert(key, value);
        }
        cx.notify();
    }
    pub(super) fn save_setup(&mut self, cx: &mut Context<Self>) {
        if self.preference.saving {
            return;
        }
        let Some(identity) = self.preference.identity.clone() else {
            return;
        };
        let groups = match self.preference.model.partition() {
            Ok(v) => v,
            Err(_) => {
                self.error = Some("Complete the required preferences.");
                cx.notify();
                return;
            }
        };
        self.preference.saving = true;
        let arguments = self
            .session
            .as_ref()
            .and_then(|s| s.root.props.get("argumentValues"))
            .cloned()
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();
        let t = self.transport.clone();
        let window = self.window;
        let key = self.session.as_ref().map(|s| s.key.clone());
        self.next_ticket += 1;
        let ticket = self.next_ticket;
        let expected = identity.clone();
        let job = cx.spawn(async move |this, cx| {
            let result = gpui_tokio::Tokio::spawn(cx, async move {
                for (command, values) in groups {
                    let v = t
                        .send(ExtensionRequest::SavePreferences {
                            extension: identity.0.clone(),
                            command,
                            values,
                        })
                        .await?;
                    success(&v)?;
                }
                t.send(ExtensionRequest::Run {
                    extension: identity.0,
                    command: identity.1,
                    arguments,
                })
                .await
            })
            .await;
            let _ = cx.update_window(window, |_, w, cx| {
                let _ = this.update(cx, |v, cx| {
                    v.auxiliary.remove(&ticket);
                    if v.preference.identity.as_ref() != Some(&expected)
                        || v.session.as_ref().map(|s| &s.key) != key.as_ref()
                    {
                        return;
                    }
                    v.preference.saving = false;
                    match result {
                        Ok(Ok(reply)) if reply["ok"] == true && reply["mode"] == "no-view" => {
                            // The command already ran. Present its actual fresh feedback;
                            // never ask the user to repeat its side effects.
                            let effects: Vec<tezbar_ui_model::extensions::WireEffect> =
                                serde_json::from_value(reply["effects"].clone())
                                    .unwrap_or_default();
                            let mut records =
                                tezbar_ui_model::extensions::runtime::WorkerRecords::default();
                            if let Some(message) = records
                                .notice(&effects, reply["effectSequence"].as_u64())
                                .or_else(|| {
                                    reply["message"]
                                        .as_str()
                                        .filter(|s| !s.is_empty())
                                        .map(str::to_owned)
                                })
                            {
                                v.emit_effect(ExtensionEffect::Notice(message), cx);
                            }
                            v.emit_effect(ExtensionEffect::Back, cx);
                        }
                        Ok(Ok(reply)) if reply["ok"] == true => {
                            // This internal replacement consumes no parent route nonce.
                            let delivered = v.opened.take();
                            let replacement = v.activate(reply, delivered.unwrap_or(0), w, cx);
                            v.opened = delivered;
                            if replacement.is_err() {
                                v.error = Some("The extension returned an invalid view.");
                            }
                        }
                        _ => v.error = Some("Preferences could not be saved. Try again."),
                    };
                    cx.notify();
                });
            });
        });
        self.auxiliary.insert(ticket, job);
        cx.notify();
    }
}
