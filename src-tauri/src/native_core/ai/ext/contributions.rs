//! Declarative provider contributions join the existing configuration only after
//! explicit enablement. No manifest is evaluated as code.
use super::*;

impl AiCore {
    pub(in crate::native_core::ai) async fn extension_config(
        &self,
        mut cfg: Value,
        cwd: Option<&std::path::Path>,
    ) -> CoreResult {
        let data = self.ext_read().await?;
        if !cfg["customProviders"].is_array() {
            cfg["customProviders"] = json!([])
        }
        for extension in data["extensions"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["enabled"] == true)
        {
            if let Some(folder) = extension["projectPath"].as_str() {
                if !cwd
                    .is_some_and(|cwd| runtime::policy::within(cwd, std::path::Path::new(folder)))
                {
                    continue;
                }
            }
            for provider in extension["providers"].as_array().into_iter().flatten() {
                let id = s(&provider["id"]);
                if cfg["customProviders"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|p| p["id"] == id)
                {
                    continue;
                }
                let mut provider = provider.clone();
                provider["_extensionId"] = extension["id"].clone();
                if !cfg["providerModels"].is_object() {
                    cfg["providerModels"] = json!({})
                }
                if cfg["providerModels"].get(id).is_none() {
                    cfg["providerModels"][id] = provider["models"].clone()
                }
                cfg["customProviders"]
                    .as_array_mut()
                    .unwrap()
                    .push(provider);
            }
        }
        Ok(cfg)
    }
    pub(in crate::native_core::ai) async fn extension_provider_headers(
        &self,
        provider: &Value,
    ) -> Result<serde_json::Map<String, Value>, String> {
        let values = if let Some(id) = provider["_extensionId"].as_str() {
            let core = self.clone();
            let namespace = key(id, &format!("provider:{}", s(&provider["id"])));
            tokio::task::spawn_blocking(move || core.inner.extensions.secrets.values(&namespace))
                .await
                .map_err(err)??
        } else {
            HashMap::new()
        };
        let mut headers = serde_json::Map::new();
        for (name, value) in provider["headers"].as_object().into_iter().flatten() {
            let value = if let Some(name) = value["secret"].as_str() {
                values.get(name).cloned().ok_or_else(|| {
                    format!(
                        "Missing provider credential: {}",
                        value["label"].as_str().unwrap_or(name)
                    )
                })?
            } else {
                value.as_str().ok_or("Invalid provider header")?.to_string()
            };
            headers.insert(name.clone(), json!(value));
        }
        Ok(headers)
    }
}
