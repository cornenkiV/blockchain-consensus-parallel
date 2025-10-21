use crate::blockchain::{Block, Blockchain};
use crate::utils::{get_config_suffix, save_blockchain};
use rand::Rng;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::error::Error;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Validator {
    pub id: u32,
    pub stake: u64,
    pub address: String,
}

impl Validator {
    pub fn new(id: u32, stake: u64) -> Self {
        Validator {
            id,
            stake,
            address: format!("validator_{}", id),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub from: String,
    pub to: String,
    pub amount: u64,
    pub signature: String,
}

impl Transaction {
    pub fn new(from: String, to: String, amount: u64) -> Self {
        let signature = format!("sig_{}_{}_{}", from, to, amount);
        Transaction {
            from,
            to,
            amount,
            signature,
        }
    }

    pub fn verify_signature(&self) -> bool {
        // simulate expensive signature verification with hash iterations
        // each transaction does random amount of work, simulates variance
        let mut rng = rand::thread_rng();
        let iterations = rng.gen_range(1000..1500);

        let mut data = format!("{}{}{}", self.from, self.to, self.amount);
        for _ in 0..iterations {
            let mut hasher = Sha256::new();
            hasher.update(data.as_bytes());
            data = format!("{:x}", hasher.finalize());
        }
        !self.signature.is_empty()
    }

    // mock
    pub fn check_balance(&self) -> bool {
        true
    }

    pub fn validate(&self) -> bool {
        self.verify_signature() && self.check_balance()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub validator_id: u32,
    pub block_index: usize,
    pub transactions_validated: usize,
    pub validation_time_ms: f64,
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorStats {
    pub validator_id: u32,
    pub stake: u64,
    pub times_selected: usize,
    pub blocks_validated: usize,
    pub total_validation_time_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PosPerformanceMetrics {
    pub total_blocks: usize,
    pub total_transactions: usize,
    pub total_time_seconds: f64,
    pub throughput_tps: f64,
    pub avg_block_validation_time_ms: f64,
    pub num_validators: usize,
}

pub fn generate_transactions(_block_index: usize, num_transactions: usize) -> Vec<Transaction> {
    let mut rng = rand::thread_rng();
    (0..num_transactions)
        .map(|_| {
            let from = format!("user_{}", rng.gen_range(0..100));
            let to = format!("user_{}", rng.gen_range(0..100));
            let amount = rng.gen_range(1..1000);
            Transaction::new(from, to, amount)
        })
        .collect()
}

pub fn select_validator_weighted(validators: &[Validator]) -> &Validator {
    let mut rng = rand::thread_rng();
    let total_stake: u64 = validators.iter().map(|v| v.stake).sum();
    let mut random_stake = rng.gen_range(0..total_stake);

    for validator in validators {
        if random_stake < validator.stake {
            return validator;
        }
        random_stake -= validator.stake;
    }

    &validators[0]
}

fn validate_block_parallel(
    transactions: &[Transaction],
    validator_id: u32,
    block_ready: Arc<AtomicBool>,
) -> (f64, u32, bool) {
    let start = Instant::now();

    if block_ready.load(Ordering::Relaxed) {
        return (start.elapsed().as_secs_f64() * 1000.0, validator_id, false);
    }

    let all_valid = transactions.iter().all(|tx| {
        if block_ready.load(Ordering::Relaxed) {
            return false;
        }
        tx.validate()
    });

    let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;

    if all_valid && !block_ready.swap(true, Ordering::SeqCst) {
        return (elapsed_ms, validator_id, true);
    }

    (elapsed_ms, validator_id, false)
}

pub fn run_pos_consensus(
    num_validators: usize,
    num_blocks: usize,
    transactions_per_block: usize,
) -> Result<(), Box<dyn Error>> {
    println!("==========================================================");
    println!("Proof-of-Stake Consensus");
    println!("==========================================================");
    println!();
    println!("Configuration:");
    println!("  Validators: {}", num_validators);
    println!("  Blocks to create: {}", num_blocks);
    println!("  Transactions per block: {}", transactions_per_block);
    println!();

    let mut rng = rand::thread_rng();
    let validators: Vec<Validator> = (0..num_validators)
        .map(|i| {
            let stake = rng.gen_range(100..1000);
            Validator::new(i as u32, stake)
        })
        .collect();

    println!("=== Validators ===");
    for v in &validators {
        println!("Validator {}: stake = {} ({})", v.id, v.stake, v.address);
    }
    println!();

    let start_time = Instant::now();
    let mut blockchain = Blockchain::new(0); //pos doesnt have difficulty
    let mut all_validation_results = Vec::new();
    let mut validator_selection_count: Vec<usize> = vec![0; num_validators];

    let mut validator_blocks_validated: Vec<usize> = vec![0; num_validators];
    let mut validator_total_time: Vec<f64> = vec![0.0; num_validators];

    for block_idx in 1..=num_blocks {
        println!("\n--- Block {} ---", block_idx);

        let selected_validator = select_validator_weighted(&validators);
        validator_selection_count[selected_validator.id as usize] += 1;

        println!(
            "Selected validator {} (stake: {}) to propose block",
            selected_validator.id, selected_validator.stake
        );

        let transactions = generate_transactions(block_idx, transactions_per_block);

        let previous_hash = blockchain.last_block().hash.clone();
        let timestamp = chrono::Utc::now().timestamp();

        let block_ready = Arc::new(AtomicBool::new(false));
        let validation_results: Vec<(f64, u32, bool)> = validators
            .par_iter()
            .map(|validator| {
                validate_block_parallel(&transactions, validator.id, Arc::clone(&block_ready))
            })
            .collect();

        let mut winner_id = 0;
        let mut fastest_time = f64::MAX;

        for (time_ms, validator_id, success) in &validation_results {
            if *success {
                fastest_time = *time_ms;
                winner_id = *validator_id;
            }
        }

        println!(
            "Validator {} completed validation first in {:.2}ms",
            winner_id, fastest_time
        );

        for (time_ms, validator_id, success) in validation_results {
            let result = ValidationResult {
                validator_id,
                block_index: block_idx,
                transactions_validated: if success { transactions_per_block } else { 0 },
                validation_time_ms: time_ms,
                success,
            };
            all_validation_results.push(result);

            validator_blocks_validated[validator_id as usize] += 1;
            validator_total_time[validator_id as usize] += time_ms;
        }

        let block_data = format!(
            "Block {} proposed by validator {} with {} transactions",
            block_idx, selected_validator.id, transactions_per_block
        );

        let block = Block::new(
            previous_hash,
            timestamp,
            selected_validator.id as u64, // pos doesnt have nonce so i used it for validator id
            block_data,
        );
        blockchain.add_block(block);

        println!(
            "Block {} added to blockchain (hash: {}...)",
            block_idx,
            &blockchain.last_block().hash[..16]
        );
    }

    let total_time = start_time.elapsed().as_secs_f64();
    let total_transactions = num_blocks * transactions_per_block;
    let throughput = total_transactions as f64 / total_time;
    let avg_block_time = total_time / num_blocks as f64;

    println!("\n==================================================");
    println!("PoS Summary");
    println!("\n==================================================");
    println!("Total blocks created: {}", num_blocks);
    println!("Total transactions: {}", total_transactions);
    println!("Total time: {:.3}s", total_time);
    println!("Throughput: {:.2} TPS", throughput);
    println!("Avg block validation: {:.2}ms", avg_block_time * 1000.0);
    println!();

    println!("=== Validator Statistics ===");
    let mut validator_stats = Vec::new();
    for (idx, validator) in validators.iter().enumerate() {
        let times_selected = validator_selection_count[idx];
        let blocks_validated = validator_blocks_validated[idx];
        let total_time_ms = validator_total_time[idx];

        let selection_rate = if num_blocks > 0 {
            (times_selected as f64 / num_blocks as f64) * 100.0
        } else {
            0.0
        };

        println!(
            "Validator {} ({} stake): selected {}  times ({:.1}%), validated {} blocks, avg time {:.2}ms",
            validator.id,
            validator.stake,
            times_selected,
            selection_rate,
            blocks_validated,
            if blocks_validated > 0 {
                total_time_ms / blocks_validated as f64
            } else {
                0.0
            }
        );

        validator_stats.push(ValidatorStats {
            validator_id: validator.id,
            stake: validator.stake,
            times_selected,
            blocks_validated,
            total_validation_time_ms: total_time_ms,
        });
    }
    println!();

    println!("Saving results...");
    let config_suffix = get_config_suffix(
        num_validators,
        num_blocks,
        Some(transactions_per_block),
        None,
    );

    crate::utils::ensure_output_dir()?;
    let validation_filename = format!("pos_validation_rust_{}.json", config_suffix);
    let validation_path = format!("output/{}", validation_filename);
    let validation_json = serde_json::to_string_pretty(&all_validation_results)?;
    std::fs::write(&validation_path, validation_json)?;
    println!("  - {}", validation_filename);

    let blockchain_filename = format!("pos_blockchain_rust_{}.json", config_suffix);
    save_blockchain(&blockchain, &blockchain_filename)?;
    println!("  - {}", blockchain_filename);

    let stats_filename = format!("pos_validator_stats_rust_{}.csv", config_suffix);
    let stats_path = format!("output/{}", stats_filename);
    let mut wtr = csv::Writer::from_path(&stats_path)?;
    wtr.write_record(&[
        "validator_id",
        "stake",
        "times_selected",
        "blocks_validated",
        "total_validation_time_ms",
    ])?;
    for stat in &validator_stats {
        wtr.write_record(&[
            stat.validator_id.to_string(),
            stat.stake.to_string(),
            stat.times_selected.to_string(),
            stat.blocks_validated.to_string(),
            format!("{:.2}", stat.total_validation_time_ms),
        ])?;
    }
    wtr.flush()?;
    println!("  - {}", stats_filename);

    let metrics = PosPerformanceMetrics {
        total_blocks: num_blocks,
        total_transactions,
        total_time_seconds: total_time,
        throughput_tps: throughput,
        avg_block_validation_time_ms: avg_block_time * 1000.0,
        num_validators,
    };
    let perf_filename = format!("pos_performance_rust_{}.csv", config_suffix);
    let perf_path = format!("output/{}", perf_filename);
    let mut wtr = csv::Writer::from_path(&perf_path)?;
    wtr.write_record(&[
        "total_blocks",
        "total_transactions",
        "total_time_seconds",
        "throughput_tps",
        "avg_block_validation_time_ms",
        "num_validators",
    ])?;
    wtr.write_record(&[
        metrics.total_blocks.to_string(),
        metrics.total_transactions.to_string(),
        format!("{:.6}", metrics.total_time_seconds),
        format!("{:.2}", metrics.throughput_tps),
        format!("{:.2}", metrics.avg_block_validation_time_ms),
        metrics.num_validators.to_string(),
    ])?;
    wtr.flush()?;
    println!("  - {}", perf_filename);

    Ok(())
}
