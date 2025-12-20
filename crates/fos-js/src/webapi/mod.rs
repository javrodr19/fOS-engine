//! Web APIs Module
//!
//! URL, TextEncoder, Blob, AbortController, Geolocation, Notifications, Sensors, Push.

pub mod url;
pub mod encoding;
pub mod blob;
pub mod abort;
pub mod geolocation;
pub mod notifications;
pub mod permissions;
pub mod sensors;
pub mod push;

pub use url::{JsUrl, JsUrlSearchParams};
pub use encoding::{TextEncoder, TextDecoder};
pub use blob::{Blob, File, FileReader};
pub use abort::{AbortController, AbortSignal};
pub use geolocation::{Geolocation, Position, Coordinates};
pub use notifications::{Notification, NotificationPermission};
pub use permissions::{Permissions, PermissionState, PermissionDescriptor};
pub use sensors::{DeviceOrientationEvent, DeviceMotionEvent, Accelerometer, Gyroscope, Sensor};
pub use push::{PushManager, PushSubscription};
