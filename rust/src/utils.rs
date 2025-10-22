use crate::blockchain::Blockchain;
use csv::Writer;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fs::{self, File};
use std::io::Write as IoWrite;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MiningProgress {
    pub block_number: usize,
    pub nonce: u64,
    pub hash: String,
    pub nonces_tested: u64,
    pub time_seconds: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub total_blocks: usize,
    pub difficulty: usize,
    pub total_time_seconds: f64,
    pub total_nonces_tested: u64,
    pub hash_rate: f64,
    //pub avg_time_per_block: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadPerformance {
    pub thread_id: usize,
    pub blocks_found: usize,
    pub total_attempts: u64,
    pub total_time_seconds: f64,
}

pub fn ensure_output_dir() -> Result<(), Box<dyn Error>> {
    let output_dir = "output";
    if !Path::new(output_dir).exists() {
        fs::create_dir_all(output_dir)?;
    }
    Ok(())
}

pub fn get_config_suffix(
    difficulty: usize,
    num_blocks: usize,
    txs_per_block: Option<usize>,
    num_workers: Option<usize>,
) -> String {
    let mut parts = vec![format!("d{}", difficulty), format!("b{}", num_blocks)];

    if let Some(txs) = txs_per_block {
        parts.push(format!("t{}", txs));
    }

    if let Some(workers) = num_workers {
        parts.push(format!("w{}", workers));
    }

    parts.join("_")
}

pub fn save_mining_progress(
    progress: &[MiningProgress],
    filename: &str,
) -> Result<(), Box<dyn Error>> {
    ensure_output_dir()?;
    let path = format!("output/{}", filename);
    let json = serde_json::to_string_pretty(&progress)?;
    let mut file = File::create(path)?;
    file.write_all(json.as_bytes())?;
    Ok(())
}

pub fn save_blockchain(blockchain: &Blockchain, filename: &str) -> Result<(), Box<dyn Error>> {
    ensure_output_dir()?;
    let path = format!("output/{}", filename);
    let json = serde_json::to_string_pretty(&blockchain)?;
    let mut file = File::create(path)?;
    file.write_all(json.as_bytes())?;
    Ok(())
}

pub fn save_performance_csv(
    metrics: &PerformanceMetrics,
    filename: &str,
) -> Result<(), Box<dyn Error>> {
    ensure_output_dir()?;
    let path = format!("output/{}", filename);
    let mut writer = Writer::from_path(path)?;

    writer.write_record(&[
        "total_blocks",
        "difficulty",
        "total_time_seconds",
        "total_nonces_tested",
        "hash_rate",
        //"avg_time_per_block",
    ])?;

    writer.write_record(&[
        metrics.total_blocks.to_string(),
        metrics.difficulty.to_string(),
        format!("{:.6}", metrics.total_time_seconds),
        metrics.total_nonces_tested.to_string(),
        format!("{:.2}", metrics.hash_rate),
        //format!("{:.6}", metrics.avg_time_per_block),
    ])?;

    writer.flush()?;
    Ok(())
}

pub fn save_thread_performance(
    thread_data: &[ThreadPerformance],
    filename: &str,
) -> Result<(), Box<dyn Error>> {
    ensure_output_dir()?;
    let path = format!("output/{}", filename);
    let json = serde_json::to_string_pretty(&thread_data)?;
    let mut file = File::create(path)?;
    file.write_all(json.as_bytes())?;
    Ok(())
}

pub fn format_hash_rate(hash_rate: f64) -> String {
    if hash_rate >= 1_000_000.0 {
        format!("{:.2} MH/s", hash_rate / 1_000_000.0)
    } else if hash_rate >= 1_000.0 {
        format!("{:.2} KH/s", hash_rate / 1_000.0)
    } else {
        format!("{:.2} H/s", hash_rate)
    }
}

pub fn print_progress(block_index: usize, total_blocks: usize, attempts: u64, elapsed: f64) {
    let hash_rate = attempts as f64 / elapsed;
    println!(
        "Mining block {}/{}  - Attempts: {} - Hash rate: {} - Elapsed: {:.2}s",
        block_index,
        total_blocks,
        attempts,
        format_hash_rate(hash_rate),
        elapsed
    );
}

pub fn create_block_data(index: usize, transactions: &[String]) -> String {
    if transactions.is_empty() {
        format!("Block {} data", index)
    } else {
        format!(
            "Block {} with {} transactions: {}",
            index,
            transactions.len(),
            transactions.join(", ")
        )
    }
}

pub fn generate_transactions(block_index: usize, num_transactions: usize) -> Vec<String> {
    (0..num_transactions)
        .map(|i| {
            format!(
                "TX{}-{}: Alice -> Bob: {} BTC",
                block_index,
                i,
                (i + 1) as f64 * 0.1
            )
        })
        .collect()
}
