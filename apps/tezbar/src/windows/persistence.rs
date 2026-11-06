//! One serial writer owns position snapshots; config updates merge only the owned keys.
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};
use tezbar_host::{
    layout::Profile,
    placement::{self, PersistedWindowPosition},
};
struct Snapshot {
    positions: BTreeMap<String, PersistedWindowPosition>,
    layouts: BTreeMap<String, Profile>,
}
pub struct Writer {
    sender: Option<tokio::sync::mpsc::UnboundedSender<Snapshot>>,
    task: Option<tokio::task::JoinHandle<Result<(), String>>>,
}
impl Writer {
    pub fn new(config: Arc<tezbar_core::common::ConfigStore>, layout: PathBuf) -> Self {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Snapshot>();
        let task = tokio::spawn(async move {
            while let Some(mut snapshot) = rx.recv().await {
                while let Ok(latest) = rx.try_recv() {
                    snapshot = latest;
                }
                let config = config.clone();
                let layout = layout.clone();
                tokio::task::spawn_blocking(move || {
                    config.update(|value| {
                        for (id, p) in snapshot.positions {
                            placement::update_window_position_config(value, &id, p);
                        }
                        Ok(())
                    })?;
                    tezbar_core::common::write_json_atomic(
                        &layout,
                        &serde_json::json!(snapshot.layouts),
                    )
                })
                .await
                .map_err(|e| e.to_string())??;
            }
            Ok(())
        });
        Self {
            sender: Some(tx),
            task: Some(task),
        }
    }
    pub fn snapshot(
        &self,
        positions: BTreeMap<String, PersistedWindowPosition>,
        layouts: BTreeMap<String, Profile>,
    ) -> Result<(), String> {
        self.sender
            .as_ref()
            .ok_or("Position writer closed")?
            .send(Snapshot { positions, layouts })
            .map_err(|e| e.to_string())
    }
    pub fn drain(&mut self) -> Option<tokio::task::JoinHandle<Result<(), String>>> {
        self.sender.take();
        self.task.take()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn serial_snapshots_keep_latest_position_without_overwriting_other_settings() {
        let root = tempfile::tempdir().unwrap();
        let config = Arc::new(tezbar_core::common::ConfigStore::new(
            root.path().join("config.json"),
        ));
        config
            .patch(&serde_json::json!({"provider":"keep"}))
            .unwrap();
        let mut writer = Writer::new(config.clone(), root.path().join("layouts.json"));
        for x in 0..30 {
            writer
                .snapshot(
                    BTreeMap::from([(
                        "monitor".into(),
                        PersistedWindowPosition {
                            x: x as f64,
                            y: 20.,
                        },
                    )]),
                    BTreeMap::from([("monitor".into(), Profile::default())]),
                )
                .unwrap();
        }
        writer.drain().unwrap().await.unwrap().unwrap();
        let value = config.read().unwrap();
        assert_eq!(value["provider"], "keep");
        assert_eq!(
            placement::monitor_positions_from_config(&value)["monitor"].x,
            29.
        );
        assert!(root.path().join("layouts.json").is_file());
    }
}
