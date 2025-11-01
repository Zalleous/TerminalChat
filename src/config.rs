/// Configuration constants for the application

/// Maximum number of concurrent client connections to the server
pub const MAX_CONNECTIONS: usize = 100;

/// Maximum message size in bytes (1 MB)
pub const MAX_MESSAGE_SIZE: usize = 1024 * 1024;

/// Broadcast channel capacity
pub const BROADCAST_CHANNEL_SIZE: usize = 100;

/// Default server port
pub const DEFAULT_PORT: u16 = 8080;

/// Default download directory for received files
pub const DEFAULT_DOWNLOAD_DIR: &str = "downloads";
