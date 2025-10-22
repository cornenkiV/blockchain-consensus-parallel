use crate::blockchain::{Block, Blockchain};
use crate::utils::{
    MiningProgress, PerformanceMetrics, create_block_data, generate_transactions,
    get_config_suffix, print_progress, save_blockchain, save_mining_progress, save_performance_csv,
};
use std::error::Error;
use std::time::Instant;

fn mine_block(
    previous_hash: String,
    timestamp: i64,
    data: String,
    difficulty: usize,
) -> (Block, u64) {
    let mut nonce = 0u64;
    let mut attempts = 0u64;

    loop {
        let block = Block::new(previous_hash.clone(), timestamp, nonce, data.clone());
        attempts += 1;

        if block.meets_difficulty(difficulty) {
            return (block, attempts);
        }

        nonce += 1;
    }
}

pub fn run_sequential_mining(
    num_blocks: usize,
    difficulty: usize,
    transactions_per_block: usize,
) -> Result<(), Box<dyn Error>> {
    println!("=== Sequential ProofOfWork Mining ===");
    println!("Blocks to mine: {}", num_blocks);
    println!("Difficulty: {}", difficulty);
    println!("Transactions per block: {}", transactions_per_block);
    println!();

    let start_time = Instant::now();
    let mut blockchain = Blockchain::new(difficulty);
    let mut mining_progress = Vec::new();
    let mut total_attempts = 0u64;

    for i in 1..=num_blocks {
        println!("Mining block {}...", i);
        let block_start = Instant::now();

        let transactions = generate_transactions(i, transactions_per_block);
        let block_data = create_block_data(i, &transactions);

        let previous_hash = blockchain.last_block().hash.clone();
        let timestamp = chrono::Utc::now().timestamp();

        let (block, attempts) = mine_block(previous_hash, timestamp, block_data, difficulty);
        let block_time = block_start.elapsed().as_secs_f64();

        total_attempts += attempts;

        let progress = MiningProgress {
            block_number: i,
            nonce: block.nonce,
            hash: block.hash.clone(),
            nonces_tested: attempts,
            time_seconds: block_time,
        };
        mining_progress.push(progress);

        println!(
            "  Found, Nonce: {} | Hash: {}... | Attempts: {} | Time: {:.3}s",
            block.nonce,
            &block.hash[..16],
            attempts,
            block_time
        );

        blockchain.add_block(block);

        let elapsed = start_time.elapsed().as_secs_f64();
        print_progress(i, num_blocks, total_attempts, elapsed);
        println!();
    }

    let total_time = start_time.elapsed().as_secs_f64();
    let hash_rate = total_attempts as f64 / total_time;
    let avg_time_per_block = total_time / num_blocks as f64;

    println!("=== Mining Complete ===");
    println!("Total blocks mined: {}", num_blocks);
    println!("Total time: {:.3}s", total_time);
    println!("Total attempts: {}", total_attempts);
    println!("Hash rate: {:.2} H/s", hash_rate);
    println!("Average time per block: {:.3}s", avg_time_per_block);
    println!("Blockchain valid: {}", blockchain.is_valid());
    println!();

    let config_suffix =
        get_config_suffix(difficulty, num_blocks, Some(transactions_per_block), None);

    println!("Saving results...");
    let mining_filename = format!("pow_mining_sequential_rust_{}.json", config_suffix);
    let blockchain_filename = format!("pow_blockchain_sequential_rust_{}.json", config_suffix);
    let performance_filename = format!("pow_performance_sequential_rust_{}.csv", config_suffix);

    save_mining_progress(&mining_progress, &mining_filename)?;
    save_blockchain(&blockchain, &blockchain_filename)?;

    let metrics = PerformanceMetrics {
        total_blocks: num_blocks,
        difficulty,
        total_time_seconds: total_time,
        total_nonces_tested: total_attempts,
        hash_rate,
        //avg_time_per_block,
    };
    save_performance_csv(&metrics, &performance_filename)?;

    println!("Results saved to output/");
    println!("  - {}", mining_filename);
    println!("  - {}", blockchain_filename);
    println!("  - {}", performance_filename);

    Ok(())
}
