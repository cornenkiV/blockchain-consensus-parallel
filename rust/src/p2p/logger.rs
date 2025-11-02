use serde::{Deserialize, Serialize};
use std::fs::{File, create_dir_all};
use std::io::Write;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event_type")]
pub enum NetworkEvent {
    //connection events
    PeerConnected {
        node_id: String,
        address: String,
        timestamp: u64,
    },
    PeerDisconnected {
        node_id: String,
        timestamp: u64,
    },

    //mining events
    MiningRoundStarted {
        round_number: u64,
        timestamp: u64,
    },
    MiningRoundCompleted {
        round_number: u64,
        winner_id: String,
        duration_ms: u64,
        timestamp: u64,
    },
    MiningStarted {
        node_id: String,
        timestamp: u64,
    },
    MiningStopped {
        node_id: String,
        reason: String,
        timestamp: u64,
    },

    //block events
    BlockReceived {
        block_height: usize,
        block_hash: String,
        miner_id: String,
        timestamp: u64,
    },
    BlockAdded {
        block_height: usize,
        block_hash: String,
        timestamp: u64,
    },
    BlockRejected {
        block_hash: String,
        reason: String,
        timestamp: u64,
    },
    BlockBroadcast {
        block_height: usize,
        block_hash: String,
        timestamp: u64,
    },

    //consensus events
    ChainReorganization {
        old_height: usize,
        new_height: usize,
        timestamp: u64,
    },
    ChainSyncRequested {
        requesting_node: String,
        timestamp: u64,
    },
    ChainSyncReceived {
        chain_length: usize,
        timestamp: u64,
    },
    ForkDetected {
        block_height: usize,
        timestamp: u64,
    },

    //transaction events
    TransactionReceived {
        tx_hash: String,
        timestamp: u64,
    },
    TransactionAdded {
        tx_hash: String,
        timestamp: u64,
    },
    TransactionBroadcast {
        tx_hash: String,
        timestamp: u64,
    },
}

impl NetworkEvent {
    pub fn timestamp(&self) -> u64 {
        match self {
            NetworkEvent::PeerConnected { timestamp, .. } => *timestamp,
            NetworkEvent::PeerDisconnected { timestamp, .. } => *timestamp,
            NetworkEvent::MiningRoundStarted { timestamp, .. } => *timestamp,
            NetworkEvent::MiningRoundCompleted { timestamp, .. } => *timestamp,
            NetworkEvent::MiningStarted { timestamp, .. } => *timestamp,
            NetworkEvent::MiningStopped { timestamp, .. } => *timestamp,
            NetworkEvent::BlockReceived { timestamp, .. } => *timestamp,
            NetworkEvent::BlockAdded { timestamp, .. } => *timestamp,
            NetworkEvent::BlockRejected { timestamp, .. } => *timestamp,
            NetworkEvent::BlockBroadcast { timestamp, .. } => *timestamp,
            NetworkEvent::ChainReorganization { timestamp, .. } => *timestamp,
            NetworkEvent::ChainSyncRequested { timestamp, .. } => *timestamp,
            NetworkEvent::ChainSyncReceived { timestamp, .. } => *timestamp,
            NetworkEvent::ForkDetected { timestamp, .. } => *timestamp,
            NetworkEvent::TransactionReceived { timestamp, .. } => *timestamp,
            NetworkEvent::TransactionAdded { timestamp, .. } => *timestamp,
            NetworkEvent::TransactionBroadcast { timestamp, .. } => *timestamp,
        }
    }
}

#[derive(Clone)]
pub struct NetworkLogger {
    events: Arc<Mutex<Vec<NetworkEvent>>>,
}

impl NetworkLogger {
    pub fn new() -> Self {
        Self {
            events: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn log(&self, event: NetworkEvent) {
        let mut events = self.events.lock().unwrap();
        events.push(event);
    }

    pub fn get_events(&self) -> Vec<NetworkEvent> {
        self.events.lock().unwrap().clone()
    }

    pub fn count(&self) -> usize {
        self.events.lock().unwrap().len()
    }

    pub fn clear(&self) {
        self.events.lock().unwrap().clear();
    }

    pub fn export_json(&self, filename: &str) -> Result<(), Box<dyn std::error::Error>> {
        create_dir_all("p2pstats")?;

        let path = format!("p2pstats/{}", filename);

        let events = self.events.lock().unwrap();
        let json = serde_json::to_string_pretty(&*events)?;
        let mut file = File::create(&path)?;
        file.write_all(json.as_bytes())?;
        println!("Exported {} events to {}", events.len(), path);

        Ok(())
    }

    pub fn export_csv(&self, filename: &str) -> Result<(), Box<dyn std::error::Error>> {
        create_dir_all("p2pstats")?;

        let path = format!("p2pstats/{}", filename);

        let events = self.events.lock().unwrap();
        let mut file = File::create(&path)?;

        writeln!(file, "timestamp,event_type,details")?;

        for event in events.iter() {
            let timestamp = event.timestamp();
            let (event_type, details) = match event {
                NetworkEvent::PeerConnected {
                    node_id, address, ..
                } => (
                    "peer_connected",
                    format!("node_id={},address={}", node_id, address),
                ),
                NetworkEvent::PeerDisconnected { node_id, .. } => {
                    ("peer_disconnected", format!("node_id={}", node_id))
                }
                NetworkEvent::MiningRoundStarted { round_number, .. } => {
                    ("mining_round_started", format!("round={}", round_number))
                }
                NetworkEvent::MiningRoundCompleted {
                    round_number,
                    winner_id,
                    duration_ms,
                    ..
                } => (
                    "mining_round_completed",
                    format!(
                        "round={},winner={},duration_ms={}",
                        round_number, winner_id, duration_ms
                    ),
                ),
                NetworkEvent::MiningStarted { node_id, .. } => {
                    ("mining_started", format!("node_id={}", node_id))
                }
                NetworkEvent::MiningStopped {
                    node_id, reason, ..
                } => (
                    "mining_stopped",
                    format!("node_id={},reason={}", node_id, reason),
                ),
                NetworkEvent::BlockReceived {
                    block_height,
                    block_hash,
                    miner_id,
                    ..
                } => (
                    "block_received",
                    format!(
                        "height={},hash={},miner={}",
                        block_height, block_hash, miner_id
                    ),
                ),
                NetworkEvent::BlockAdded {
                    block_height,
                    block_hash,
                    ..
                } => (
                    "block_added",
                    format!("height={},hash={}", block_height, block_hash),
                ),
                NetworkEvent::BlockRejected {
                    block_hash, reason, ..
                } => (
                    "block_rejected",
                    format!("hash={},reason={}", block_hash, reason),
                ),
                NetworkEvent::BlockBroadcast {
                    block_height,
                    block_hash,
                    ..
                } => (
                    "block_broadcast",
                    format!("height={},hash={}", block_height, block_hash),
                ),
                NetworkEvent::ChainReorganization {
                    old_height,
                    new_height,
                    ..
                } => (
                    "chain_reorganization",
                    format!("old_height={},new_height={}", old_height, new_height),
                ),
                NetworkEvent::ChainSyncRequested {
                    requesting_node, ..
                } => ("chain_sync_requested", format!("node={}", requesting_node)),
                NetworkEvent::ChainSyncReceived { chain_length, .. } => {
                    ("chain_sync_received", format!("length={}", chain_length))
                }
                NetworkEvent::ForkDetected { block_height, .. } => {
                    ("fork_detected", format!("height={}", block_height))
                }
                NetworkEvent::TransactionReceived { tx_hash, .. } => {
                    ("transaction_received", format!("hash={}", tx_hash))
                }
                NetworkEvent::TransactionAdded { tx_hash, .. } => {
                    ("transaction_added", format!("hash={}", tx_hash))
                }
                NetworkEvent::TransactionBroadcast { tx_hash, .. } => {
                    ("transaction_broadcast", format!("hash={}", tx_hash))
                }
            };

            writeln!(file, "{},{},\"{}\"", timestamp, event_type, details)?;
        }

        println!("Exported {} events to {}", events.len(), path);
        Ok(())
    }
}

impl Default for NetworkLogger {
    fn default() -> Self {
        Self::new()
    }
}
