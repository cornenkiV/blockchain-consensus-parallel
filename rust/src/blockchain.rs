use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub previous_hash: String,
    pub timestamp: i64,
    pub nonce: u64,
    pub data: String,
    pub hash: String,
}

impl Block {
    pub fn new(previous_hash: String, timestamp: i64, nonce: u64, data: String) -> Self {
        let mut block = Block {
            previous_hash,
            timestamp,
            nonce,
            data,
            hash: String::new(),
        };
        block.hash = block.calculate_hash();
        block
    }

    pub fn calculate_hash(&self) -> String {
        let block_data = format!(
            "{}{}{}{}",
            self.previous_hash, self.timestamp, self.nonce, self.data
        );
        let mut hasher = Sha256::new();
        hasher.update(block_data.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    pub fn is_valid(&self) -> bool {
        self.hash == self.calculate_hash()
    }

    pub fn meets_difficulty(&self, difficulty: usize) -> bool {
        let prefix = "0".repeat(difficulty);
        self.hash.starts_with(&prefix)
    }
}

impl fmt::Display for Block {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Block[\n  Previous: {}\n  Timestamp: {}\n  Nonce: {}\n  Data: {}\n  Hash: {}\n]",
            &self.previous_hash[..8],
            self.timestamp,
            self.nonce,
            self.data,
            &self.hash[..8]
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Blockchain {
    pub chain: Vec<Block>,
    pub difficulty: usize,
}

impl Blockchain {
    pub fn new(difficulty: usize) -> Self {
        let genesis_block = Block::new(
            "0".to_string(),
            chrono::Utc::now().timestamp(),
            0,
            "Genesis Block".to_string(),
        );
        Blockchain {
            chain: vec![genesis_block],
            difficulty,
        }
    }

    pub fn last_block(&self) -> &Block {
        self.chain.last().unwrap()
    }

    pub fn add_block(&mut self, block: Block) {
        self.chain.push(block);
    }

    pub fn is_valid(&self) -> bool {
        for i in 1..self.chain.len() {
            let current = &self.chain[i];
            let previous = &self.chain[i - 1];

            if !current.is_valid() {
                return false;
            }

            if current.previous_hash != previous.hash {
                return false;
            }

            if !current.meets_difficulty(self.difficulty) {
                return false;
            }
        }
        true
    }

    pub fn validate_block(&self, block: &Block) -> bool {
        if !block.is_valid() {
            return false;
        }

        if !block.meets_difficulty(self.difficulty) {
            return false;
        }

        let last_block = self.last_block();
        if block.previous_hash != last_block.hash {
            return false;
        }

        true
    }
}
