use super::runtime::{Node, NodeKind};
use serde_json::{json, Value};
use std::collections::BTreeMap;
#[derive(Clone)]
pub struct Field {
    pub id: String,
    pub kind: NodeKind,
    pub server_value: Option<Value>,
    pub callback: Option<String>,
    pub error: Option<String>,
    pub required: bool,
    pub options: Vec<String>,
    pub multiple: bool,
}
#[derive(Default)]
pub struct FormState {
    fields: BTreeMap<String, Field>,
    values: BTreeMap<String, Value>,
    pub wheel: Option<[f64; 4]>,
}
impl FormState {
    pub fn values(&self) -> &BTreeMap<String, Value> {
        &self.values
    }
    pub fn fields(&self) -> impl Iterator<Item = &Field> {
        self.fields.values()
    }
    pub fn clear(&mut self) {
        self.fields.clear();
        self.values.clear();
        self.wheel = None;
    }
    pub fn reconcile(&mut self, root: &Node) {
        let mut nodes = vec![];
        root.visit(&mut nodes);
        let mut fields = BTreeMap::new();
        let mut values = BTreeMap::new();
        for (index, n) in nodes.into_iter().filter(|n| n.kind.field()).enumerate() {
            let id = n
                .string("id")
                .filter(|s| !s.trim().is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| {
                    format!(
                        "{}:{index}",
                        serde_json::to_value(n.kind).unwrap().as_str().unwrap()
                    )
                });
            let callback = n.string("actionId").map(str::to_owned);
            let mut opts = vec![];
            n.visit(&mut opts);
            let mut options = opts
                .into_iter()
                .filter(|n| matches!(n.kind, NodeKind::DropdownItem | NodeKind::TagPickerItem))
                .filter_map(|n| n.string("value").or_else(|| n.string("title")))
                .map(str::to_owned)
                .collect::<Vec<_>>();
            if let Some(data) = n.props.get("data").and_then(Value::as_array) {
                options.extend(
                    data.iter()
                        .filter_map(|v| v["value"].as_str().or_else(|| v["title"].as_str()))
                        .map(str::to_owned),
                );
            }
            let field = Field {
                id: id.clone(),
                kind: n.kind,
                server_value: n.props.get("value").cloned(),
                callback,
                error: n
                    .string("error")
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned),
                required: n.props.get("required") == Some(&Value::Bool(true)),
                options,
                multiple: n.props.get("allowMultipleSelection") == Some(&Value::Bool(true)),
            };
            let keep = self.fields.get(&id).is_some_and(|old| {
                old.kind == field.kind
                    && (field.server_value.is_none()
                        || (field.callback.is_none() && old.server_value == field.server_value))
            });
            let value = if keep {
                self.values.get(&id).cloned()
            } else {
                None
            }
            .or_else(|| field.server_value.clone())
            .unwrap_or_else(|| match field.kind {
                NodeKind::Checkbox => json!(false),
                NodeKind::FilePicker | NodeKind::TagPicker => json!([]),
                _ => json!(""),
            });
            values.insert(id.clone(), value);
            fields.insert(id, field);
        }
        self.fields = fields;
        self.values = values;
        if let Some(local) = self.wheel {
            if ["hue", "saturation", "value", "alpha"]
                .iter()
                .enumerate()
                .all(|(i, k)| {
                    self.fields
                        .get(*k)
                        .and_then(|f| f.server_value.as_ref())
                        .and_then(|v| {
                            v.as_f64()
                                .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
                        })
                        .is_some_and(|v| (v - local[i]).abs() < 0.0001)
                })
            {
                self.wheel = None;
            }
        }
    }
    pub fn multiple(&self, id: &str) -> Result<bool, &'static str> {
        let f = self.fields.get(id).ok_or("Unknown field")?;
        if f.kind != NodeKind::FilePicker {
            return Err("Field is not a file picker");
        }
        Ok(f.multiple)
    }
    pub fn change(&mut self, id: &str, value: Value) -> Result<Option<String>, &'static str> {
        let f = self.fields.get(id).ok_or("Unknown field")?;
        let valid = match f.kind {
            NodeKind::Checkbox => value.is_boolean(),
            NodeKind::TagPicker | NodeKind::FilePicker => value
                .as_array()
                .is_some_and(|a| a.iter().all(Value::is_string)),
            NodeKind::DatePicker => value.is_string() || value.is_null(),
            _ => value.is_string(),
        };
        if !valid {
            return Err("Wrong field value type");
        }
        if f.kind == NodeKind::FilePicker
            && !f.multiple
            && value.as_array().is_some_and(|v| v.len() > 1)
        {
            return Err("Only one file may be selected");
        }
        if matches!(f.kind, NodeKind::Dropdown | NodeKind::TagPicker) && !f.options.is_empty() {
            let values = if let Some(a) = value.as_array() {
                a.iter().filter_map(Value::as_str).collect::<Vec<_>>()
            } else {
                value.as_str().into_iter().collect()
            };
            if values.iter().any(|v| !f.options.iter().any(|o| o == v)) {
                return Err("Selection is not an option");
            }
        }
        self.values.insert(id.into(), value);
        Ok(f.callback.clone())
    }
    pub fn validate(&self) -> Result<(), &'static str> {
        for f in self.fields.values() {
            if f.error.is_some() {
                return Err("Correct the highlighted fields");
            }
            let v = self.values.get(&f.id);
            if f.required
                && v.is_none_or(|v| {
                    v.is_null()
                        || v.as_str().is_some_and(|s| s.trim().is_empty())
                        || v.as_array().is_some_and(|a| a.is_empty())
                })
            {
                return Err("Required fields are missing");
            }
        }
        Ok(())
    }
}
#[derive(Default)]
pub struct SubmitKey {
    pub enter: bool,
    pub command: bool,
    pub control: bool,
    pub alt: bool,
    pub shift: bool,
    pub repeat: bool,
    pub composing: bool,
    pub textarea: bool,
    pub mac: bool,
}
pub fn should_submit(k: SubmitKey) -> bool {
    if !k.enter || k.repeat || k.composing || k.alt {
        return false;
    }
    if k.command || k.control {
        return if k.mac {
            k.command && !k.control
        } else {
            k.control && !k.command
        };
    }
    !k.textarea
}
#[derive(Clone)]
pub struct PreferenceField {
    pub name: String,
    pub command: Option<String>,
    pub kind: String,
    pub required: bool,
    pub options: Vec<String>,
}
impl PreferenceField {
    pub fn key(&self) -> String {
        format!(
            "{}:{}",
            self.command.as_deref().unwrap_or("$extension"),
            self.name
        )
    }
}
#[derive(Default)]
pub struct Preferences {
    pub fields: Vec<PreferenceField>,
    values: BTreeMap<String, Value>,
}
impl Preferences {
    pub fn merge(
        setups: Vec<(
            Option<String>,
            Vec<PreferenceField>,
            BTreeMap<String, Value>,
        )>,
    ) -> Self {
        let mut out = Self::default();
        for (command, fields, values) in setups {
            for field in fields {
                if field.name.is_empty() || command.is_some() && field.command.is_none() {
                    continue;
                }
                let key = field.key();
                if out.values.contains_key(&key) {
                    continue;
                }
                out.values
                    .insert(key, values.get(&field.name).cloned().unwrap_or(Value::Null));
                out.fields.push(field);
            }
        }
        out
    }
    pub fn edit(&mut self, key: &str, value: Value) -> Result<(), &'static str> {
        if !self.fields.iter().any(|f| f.key() == key) {
            return Err("Unknown preference");
        }
        self.values.insert(key.into(), value);
        Ok(())
    }
    pub fn partition(
        &self,
    ) -> Result<BTreeMap<Option<String>, BTreeMap<String, Value>>, &'static str> {
        let mut out: BTreeMap<Option<String>, BTreeMap<String, Value>> = BTreeMap::new();
        for field in &self.fields {
            let value = self
                .values
                .get(&field.key())
                .cloned()
                .unwrap_or(Value::Null);
            if field.required
                && (value.is_null() || value.as_str().is_some_and(|v| v.trim().is_empty()))
            {
                return Err("Required preferences are missing");
            }
            out.entry(field.command.clone())
                .or_default()
                .insert(field.name.clone(), value);
        }
        Ok(out)
    }
}
#[cfg(test)]
mod tests;
