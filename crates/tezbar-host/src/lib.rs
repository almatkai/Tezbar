//! Portable desktop implementation. Nothing starts until explicitly called by a host.
pub mod input;
pub mod layout;
pub mod notifications;
pub mod placement;
#[cfg(target_os = "macos")]
pub mod timer_notifications;
pub use tezbar_core::{child_supervisor, resource_directory};

pub mod desktop;
pub mod logging;
pub mod paths;
pub mod single_instance;
