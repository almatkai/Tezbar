//! One serial writer owns position snapshots; config updates merge only the owned keys.
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};
use tezbar_host::{
    layout::Profile,
    placement::{self, PersistedWindowPosition},
};
/// Convert a snapshot with the CURRENT display identity and target scale, never last-show state.
pub fn capture(
    monitor: String,
    scale: f64,
    bounds: gpui::Bounds<gpui::Pixels>,
    zoom: f32,
) -> (String, PersistedWindowPosition, Profile) {
    (
        monitor,
        PersistedWindowPosition {
            x: f64::from(bounds.left()) * scale,
            y: f64::from(bounds.top()) * scale,
        },
        Profile {
            width: f64::from(bounds.size.width),
            height: f64::from(bounds.size.height),
            zoom: zoom as f64,
        },
    )
}
#[derive(Clone)]
struct Snapshot {
    positions: BTreeMap<String, PersistedWindowPosition>,
    layouts: BTreeMap<String, Profile>,
}
pub struct Writer {
    sender: Option<tokio::sync::watch::Sender<Option<Snapshot>>>,
    task: Option<tokio::task::JoinHandle<Result<(), String>>>,
}
impl Writer {
    pub fn new(config: Arc<tezbar_core::common::ConfigStore>, layout: PathBuf) -> Self {
        let (tx, mut rx) = tokio::sync::watch::channel::<Option<Snapshot>>(None);
        let task = tokio::spawn(async move {
            let mut latest = None;
            let mut last_error = None;
            loop {
                let closed = rx.changed().await.is_err();
                if let Some(snapshot) = rx.borrow_and_update().clone() {
                    latest = Some(snapshot);
                }
                if let Some(snapshot) = latest.clone() {
                    let mut outcome = Ok(());
                    // Retry transient failures, including final durable flush. Keep latest snapshot
                    // owned on error; a failed quit recreates the writer from Owner's same maps.
                    for attempt in 0..3 {
                        let config = config.clone();
                        let layout = layout.clone();
                        let snapshot = snapshot.clone();
                        outcome = tokio::task::spawn_blocking(move || {
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
                        .map_err(|e| e.to_string())
                        .and_then(|v| v);
                        if outcome.is_ok() {
                            break;
                        }
                        if attempt < 2 {
                            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                        }
                    }
                    last_error = outcome.err();
                    if let Some(error) = &last_error {
                        log::error!("Window layout save failed (writer remains live): {error}");
                    }
                }
                if closed {
                    return last_error.map_or(Ok(()), Err);
                }
            }
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
            .send(Some(Snapshot { positions, layouts }))
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

#[cfg(test)]
mod repair_tests {
    use super::*;
    #[tokio::test]
    async fn transient_failure_keeps_writer_alive_and_drain_retries_latest() {
        let root = tempfile::tempdir().unwrap();
        let config = Arc::new(tezbar_core::common::ConfigStore::new(
            root.path().join("config.json"),
        ));
        let layout = root.path().join("layouts.json");
        std::fs::create_dir(&layout).unwrap(); // atomic file replace must fail against directory
        let mut writer = Writer::new(config, layout.clone());
        writer
            .snapshot(
                BTreeMap::new(),
                BTreeMap::from([("first".into(), Profile::default())]),
            )
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(180)).await;
        assert!(!writer.task.as_ref().unwrap().is_finished());
        std::fs::remove_dir(&layout).unwrap();
        writer
            .snapshot(
                BTreeMap::new(),
                BTreeMap::from([("latest".into(), Profile::default())]),
            )
            .unwrap();
        writer.drain().unwrap().await.unwrap().unwrap();
        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(layout).unwrap()).unwrap();
        assert!(value.get("latest").is_some());
        assert!(value.get("first").is_none());
    }
}

#[cfg(test)]
mod monitor_repair_tests {
    use super::*;
    #[test]
    fn cross_monitor_snapshot_owns_current_identity_scale_size_and_zoom() {
        let (a, position_a, profile_a) = capture(
            "display-A".into(),
            2.,
            gpui::Bounds::new(
                gpui::point(gpui::px(20.), gpui::px(30.)),
                gpui::size(gpui::px(760.), gpui::px(640.)),
            ),
            1.,
        );
        let mut positions = BTreeMap::from([(a.clone(), position_a)]);
        let mut layouts = BTreeMap::from([(a, profile_a)]);
        let (b, position_b, profile_b) = capture(
            "display-B".into(),
            1.25,
            gpui::Bounds::new(
                gpui::point(gpui::px(-1000.), gpui::px(40.)),
                gpui::size(gpui::px(900.), gpui::px(700.)),
            ),
            1.5,
        );
        positions.insert(b.clone(), position_b);
        layouts.insert(b, profile_b);
        assert_eq!(positions["display-A"].x, 40.);
        assert_eq!(positions["display-B"].x, -1250.);
        assert_eq!(layouts["display-A"].width, 760.);
        assert_eq!(layouts["display-B"].width, 900.);
        assert_eq!(layouts["display-B"].zoom, 1.5);
    }
}
