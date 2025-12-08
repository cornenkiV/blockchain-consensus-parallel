use crate::p2p::protocol::P2PMessage;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// track messages to prevent loops
#[derive(Clone)]
pub struct MessageTracker {
    seen: Arc<Mutex<HashMap<String, Instant>>>,
    ttl: Duration,
}

impl MessageTracker {
    pub fn new(ttl_seconds: u64) -> Self {
        MessageTracker {
            seen: Arc::new(Mutex::new(HashMap::new())),
            ttl: Duration::from_secs(ttl_seconds),
        }
    }

    pub fn is_seen(&self, msg_id: &str) -> bool {
        let seen = self.seen.lock().unwrap();
        seen.contains_key(msg_id)
    }

    pub fn mark_seen(&self, msg_id: String) {
        let mut seen = self.seen.lock().unwrap();
        seen.insert(msg_id, Instant::now());
    }

    pub fn cleanup(&self) {
        let mut seen = self.seen.lock().unwrap();
        let now = Instant::now();
        seen.retain(|_, timestamp| now.duration_since(*timestamp) < self.ttl);
    }

    pub fn count(&self) -> usize {
        self.seen.lock().unwrap().len()
    }
}

/// unique message id for deduplication
pub fn compute_message_id(message: &P2PMessage) -> String {
    let serialized = serde_json::to_string(message).unwrap_or_default();

    let mut hasher = Sha256::new();
    hasher.update(serialized.as_bytes());
    let hash = hasher.finalize();

    format!("{:x}", hash)[..16].to_string()
}
