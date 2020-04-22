use serde::{Deserialize, Serialize};

use crate::db::RedisEntity;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum WsPacket {
    Join { room: String },
    AssignedUsername { username: String },

    ServerMessage { from: String, message: String, extra: Option<MessageExtra> },
    ClientMessage { message: String },

    GetInvoice { amount: u64, message: String },
    Invoice { id: String },

    UpdateViewers { viewers: usize },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MessageExtra {
    pub amount: u64,
    pub timestamp: u64,
    pub duration: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum VideoStatus {
    Scheduled {
        timestamp: u64,
    },
    Live {
        started_timestamp: u64,
        viewers: usize,
    },
    Upload {
        timestamp: u64,
    },
    Processing,
    Published {
        timestamp: u64,
        duration: f32,
        views: usize,
        variants: Vec<(usize, String, String)>,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Video {
    pub id: String,
    pub title: String,
    pub description: String,
    pub status: VideoStatus,
}

impl RedisEntity for Video {
    type Id = String;

    fn key() -> &'static str {
        "videos"
    }

    fn id(&self) -> &String {
        &self.id
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BoostMessageInvoice {
    pub id: String,
    pub message: String,
    pub from: String,
    pub room: String,
}

impl RedisEntity for BoostMessageInvoice {
    type Id = String;

    fn key() -> &'static str {
        "message_invoices"
    }

    fn id(&self) -> &String {
        &self.id
    }
}



#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Message {
    from: String,
    message: String,
}
