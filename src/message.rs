use serde::{Deserialize, Serialize};
use std::time::SystemTime;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    Text {
        username: String,
        content: String,
        timestamp: SystemTime,
    },
    File {
        username: String,
        filename: String,
        size: u64,
        data: Vec<u8>,
        timestamp: SystemTime,
    },
    UserJoined {
        username: String,
        timestamp: SystemTime,
    },
    UserLeft {
        username: String,
        timestamp: SystemTime,
    },
    System {
        content: String,
        timestamp: SystemTime,
    },
}

impl Message {
    pub fn new_text(username: String, content: String) -> Self {
        Message::Text {
            username,
            content,
            timestamp: SystemTime::now(),
        }
    }

    pub fn new_file(username: String, filename: String, data: Vec<u8>) -> Self {
        let size = data.len() as u64;
        Message::File {
            username,
            filename,
            size,
            data,
            timestamp: SystemTime::now(),
        }
    }

    pub fn new_user_joined(username: String) -> Self {
        Message::UserJoined {
            username,
            timestamp: SystemTime::now(),
        }
    }

    pub fn new_user_left(username: String) -> Self {
        Message::UserLeft {
            username,
            timestamp: SystemTime::now(),
        }
    }

    pub fn new_system(content: String) -> Self {
        Message::System {
            content,
            timestamp: SystemTime::now(),
        }
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}
