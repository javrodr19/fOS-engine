//! Web APIs Module
//!
//! URL, TextEncoder, Blob, AbortController, Geolocation, Notifications, Sensors, Push,
//! Fetch API, XMLHttpRequest.

pub mod url;
pub mod encoding;
pub mod blob;
pub mod abort;
pub mod geolocation;
pub mod notifications;
pub mod permissions;
pub mod sensors;
pub mod push;
pub mod formdata;
pub mod file_reader;
pub mod vibration;
pub mod fetch;
pub mod xhr;

pub use url::{JsUrl, JsUrlSearchParams};
pub use encoding::{TextEncoder, TextDecoder};
pub use blob::{Blob, File, FileReader as BlobFileReader};
pub use abort::{AbortController, AbortSignal};
pub use geolocation::{Geolocation, Position, Coordinates};
pub use notifications::{Notification, NotificationPermission};
pub use permissions::{Permissions, PermissionState, PermissionDescriptor};
pub use sensors::{DeviceOrientationEvent, DeviceMotionEvent, Accelerometer, Gyroscope, Sensor};
pub use push::{PushManager, PushSubscription};
pub use formdata::{FormData, FormDataValue, FileEntry};
pub use file_reader::{FileReader, FileReaderState, FileReaderResult};
pub use vibration::{VibrationController, VibrationPattern, PermissionPromptManager, PermissionType};
pub use fetch::{Request, Response, Headers, FetchManager, HttpMethod, RequestBody, FetchResult};
pub use xhr::{XMLHttpRequest, XhrManager, XhrReadyState, XhrResponseType, XhrResponse};

