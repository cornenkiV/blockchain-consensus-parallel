use crate::blockchain::Block;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::net::TcpStream;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum P2PMessage {
    Join {
        node_id: String,
        address: String,
        timestamp: u64,
    },

    /// list of connected peers
    PeerList { peers: Vec<PeerInfo> },

    /// asks for blockchain from peer
    RequestBlockchain { requester_id: String },

    /// response with full blockchain
    BlockchainSync { chain: Vec<Block> },

    /// initiate mining with block template
    MiningStart { template: BlockTemplate },

    /// signal to stop mining
    MiningStop,

    /// new block broadcast
    NewBlock { block: Block, miner_id: String },

    /// new tx broadcast
    NewTransaction {
        transaction: String,
        from_node: String,
    },

    /// heartbeat to maintain connection
    Heartbeat { node_id: String, timestamp: u64 },

    /// response to heartbeat
    Pong { node_id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub node_id: String,
    pub address: String, // IP:PORT
    pub last_seen: u64,
    pub blocks_mined: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockTemplate {
    pub previous_hash: String,
    pub transactions: Vec<String>,
    pub difficulty: usize,
    pub timestamp: i64,
    pub block_number: usize,
}

impl BlockTemplate {
    pub fn new(
        previous_hash: String,
        transactions: Vec<String>,
        difficulty: usize,
        block_number: usize,
    ) -> Self {
        BlockTemplate {
            previous_hash,
            transactions,
            difficulty,
            timestamp: chrono::Utc::now().timestamp(),
            block_number,
        }
    }

    pub fn to_block(&self, nonce: u64) -> Block {
        let data = format!(
            "Block {} with {} transactions",
            self.block_number,
            self.transactions.len()
        );

        Block::new(self.previous_hash.clone(), self.timestamp, nonce, data)
    }
}

impl P2PMessage {
    pub fn to_bytes(&self) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let json = serde_json::to_string(self)?;
        let bytes = json.as_bytes().to_vec();
        Ok(bytes)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Box<dyn std::error::Error>> {
        let json = std::str::from_utf8(bytes)?;
        let message = serde_json::from_str(json)?;
        Ok(message)
    }

    pub fn send(&self, stream: &mut TcpStream) -> Result<(), Box<dyn std::error::Error>> {
        let bytes = self.to_bytes()?;
        let len = bytes.len() as u32;

        stream.write_all(&len.to_be_bytes())?;

        stream.write_all(&bytes)?;
        stream.flush()?;

        Ok(())
    }

    pub fn receive(stream: &mut TcpStream) -> Result<Self, Box<dyn std::error::Error>> {
        let mut len_bytes = [0u8; 4];
        stream.read_exact(&mut len_bytes)?;
        let len = u32::from_be_bytes(len_bytes) as usize;

        let mut buffer = vec![0u8; len];
        stream.read_exact(&mut buffer)?;

        Self::from_bytes(&buffer)
    }
}
