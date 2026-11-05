use crate::placement::{valid_scale_factor, WINDOW_HEIGHT, WINDOW_WIDTH};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
pub const MIN_WIDTH: f64 = 480.0;
pub const MIN_HEIGHT: f64 = 360.0;
const MIN_ZOOM: f64 = 0.75;
const MAX_ZOOM: f64 = 2.0;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub width: f64,
    pub height: f64,
    pub zoom: f64,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            width: WINDOW_WIDTH,
            height: WINDOW_HEIGHT,
            zoom: 1.0,
        }
    }
}

fn finite_or(value: f64, fallback: f64) -> f64 {
    if value.is_finite() {
        value
    } else {
        fallback
    }
}

impl Profile {
    pub fn physical_size(self, dpi: f64) -> (f64, f64) {
        let dpi = valid_scale_factor(dpi);
        (self.width * dpi, self.height * dpi)
    }

    pub fn fit(self, available: (f64, f64)) -> Self {
        let zoom = finite_or(self.zoom, 1.0).clamp(MIN_ZOOM, MAX_ZOOM);
        let max_width = available.0.max(1.0);
        let max_height = available.1.max(1.0);
        Self {
            width: finite_or(self.width, WINDOW_WIDTH)
                .clamp((MIN_WIDTH * zoom).min(max_width), max_width),
            height: finite_or(self.height, WINDOW_HEIGHT)
                .clamp((MIN_HEIGHT * zoom).min(max_height), max_height),
            zoom,
        }
    }
}

pub fn profiles_from_json(raw: &str) -> BTreeMap<String, Profile> {
    serde_json::from_str::<serde_json::Value>(raw)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .map(|profiles| {
            profiles
                .into_iter()
                .filter_map(|(id, value)| {
                    serde_json::from_value(value)
                        .ok()
                        .map(|profile| (id, profile))
                })
                .collect()
        })
        .unwrap_or_default()
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub enum ResizeEdge {
    North,
    NorthEast,
    East,
    SouthEast,
    South,
    SouthWest,
    West,
    NorthWest,
}

/// Pure geometry; opposite edges remain anchored when a minimum/maximum is reached.
pub fn resized_rect(
    rect: (f64, f64, f64, f64),
    delta: (f64, f64),
    edge: ResizeEdge,
    minimum: (f64, f64),
    maximum: (f64, f64),
) -> (f64, f64, f64, f64) {
    let (mut x, mut y, width, height) = rect;
    let west = matches!(
        edge,
        ResizeEdge::West | ResizeEdge::NorthWest | ResizeEdge::SouthWest
    );
    let east = matches!(
        edge,
        ResizeEdge::East | ResizeEdge::NorthEast | ResizeEdge::SouthEast
    );
    let north = matches!(
        edge,
        ResizeEdge::North | ResizeEdge::NorthEast | ResizeEdge::NorthWest
    );
    let south = matches!(
        edge,
        ResizeEdge::South | ResizeEdge::SouthEast | ResizeEdge::SouthWest
    );
    let next_width = (width
        + if west {
            -delta.0
        } else if east {
            delta.0
        } else {
            0.0
        })
    .clamp(minimum.0.min(maximum.0), maximum.0);
    let next_height = (height
        + if north {
            -delta.1
        } else if south {
            delta.1
        } else {
            0.0
        })
    .clamp(minimum.1.min(maximum.1), maximum.1);
    if west {
        x += width - next_width;
    }
    if north {
        y += height - next_height;
    }
    (x, y, next_width, next_height)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fits_small_displays_and_invalid_values() {
        assert_eq!(
            Profile {
                width: f64::NAN,
                height: 5000.0,
                zoom: f64::INFINITY
            }
            .fit((600.0, 400.0)),
            Profile {
                width: 600.0,
                height: 400.0,
                zoom: 1.0
            }
        );
        assert_eq!(
            Profile {
                width: 1.0,
                height: 1.0,
                zoom: 2.0
            }
            .fit((800.0, 600.0)),
            Profile {
                width: 800.0,
                height: 600.0,
                zoom: 2.0
            }
        );
    }
    #[test]
    fn saved_logical_size_survives_mixed_dpi_transitions() {
        let profile = Profile {
            width: 610.0,
            height: 470.0,
            zoom: 1.25,
        };
        assert_eq!(profile.physical_size(1.0), (610.0, 470.0));
        assert_eq!(profile.physical_size(2.0), (1220.0, 940.0));
        assert_eq!(profile.physical_size(1.5), (915.0, 705.0));
        assert_eq!(profile.physical_size(f64::NAN), (610.0, 470.0));
    }

    #[test]
    fn a_corrupt_display_profile_does_not_destroy_other_displays() {
        let profiles = profiles_from_json(
            r#"{"bad":{"width":"wrong"},"good":{"width":600,"height":480,"zoom":1.5}}"#,
        );
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles["good"].width, 600.0);
        assert!(profiles_from_json("broken json").is_empty());
    }

    #[test]
    fn minimum_keeps_opposite_corner_anchored() {
        assert_eq!(
            resized_rect(
                (100.0, 200.0, 760.0, 640.0),
                (1000.0, 1000.0),
                ResizeEdge::NorthWest,
                (480.0, 360.0),
                (2000.0, 2000.0)
            ),
            (380.0, 480.0, 480.0, 360.0)
        );
    }
    #[test]
    fn all_eight_handles_resize_the_requested_edges() {
        let cases = [
            (ResizeEdge::North, (100.0, 280.0, 760.0, 560.0)),
            (ResizeEdge::NorthEast, (100.0, 280.0, 860.0, 560.0)),
            (ResizeEdge::East, (100.0, 200.0, 860.0, 640.0)),
            (ResizeEdge::SouthEast, (100.0, 200.0, 860.0, 720.0)),
            (ResizeEdge::South, (100.0, 200.0, 760.0, 720.0)),
            (ResizeEdge::SouthWest, (200.0, 200.0, 660.0, 720.0)),
            (ResizeEdge::West, (200.0, 200.0, 660.0, 640.0)),
            (ResizeEdge::NorthWest, (200.0, 280.0, 660.0, 560.0)),
        ];
        for (edge, expected) in cases {
            assert_eq!(
                resized_rect(
                    (100.0, 200.0, 760.0, 640.0),
                    (100.0, 80.0),
                    edge,
                    (480.0, 360.0),
                    (1920.0, 1080.0)
                ),
                expected,
                "{edge:?}"
            );
        }
    }

    #[test]
    fn edge_resizes_only_one_axis_and_maximum_is_enforced() {
        assert_eq!(
            resized_rect(
                (100.0, 200.0, 760.0, 640.0),
                (2000.0, 300.0),
                ResizeEdge::East,
                (480.0, 360.0),
                (1920.0, 1080.0)
            ),
            (100.0, 200.0, 1920.0, 640.0)
        );
    }
    #[test]
    fn profiles_round_trip_independently_of_dpi_and_other_displays() {
        let mut profiles = BTreeMap::new();
        profiles.insert(
            "retina".to_string(),
            Profile {
                width: 600.0,
                height: 480.0,
                zoom: 0.9,
            },
        );
        profiles.insert(
            "4k".to_string(),
            Profile {
                width: 1200.0,
                height: 900.0,
                zoom: 1.5,
            },
        );
        let restored: BTreeMap<String, Profile> =
            serde_json::from_str(&serde_json::to_string(&profiles).unwrap()).unwrap();
        assert_eq!(restored, profiles);
        assert_eq!(restored["4k"].fit((3840.0, 2160.0)).zoom, 1.5);
        assert_eq!(restored["retina"].fit((1512.0, 982.0)).width, 600.0);
    }
}
