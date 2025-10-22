use crate::blockchain::{Block, Blockchain};
use crate::utils::{
    MiningProgress, PerformanceMetrics, ThreadPerformance, create_block_data,
    generate_transactions, get_config_suffix, print_progress, save_blockchain,
    save_mining_progress, save_performance_csv, save_thread_performance,
};
use rayon::prelude::*;
use std::error::Error;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[derive(Debug, Clone)]
struct MiningResult {
    block: Block,
    #[allow(dead_code)]
    attempts: u64,
    thread_id: usize,
}

fn mine_block_parallel(
    previous_hash: String,
    timestamp: i64,
    data: String,
    difficulty: usize,
    num_workers: usize,
) -> (Block, u64, Vec<ThreadPerformance>) {
    let found = Arc::new(AtomicBool::new(false));
    let global_attempts = Arc::new(AtomicU64::new(0));

    let result: Arc<parking_lot::Mutex<Option<MiningResult>>> =
        Arc::new(parking_lot::Mutex::new(None));

    let thread_attempts: Vec<Arc<AtomicU64>> = (0..num_workers)
        .map(|_| Arc::new(AtomicU64::new(0)))
        .collect();

    let thread_times: Vec<Arc<parking_lot::Mutex<f64>>> = (0..num_workers)
        .map(|_| Arc::new(parking_lot::Mutex::new(0.0)))
        .collect();

    rayon::ThreadPoolBuilder::new()
        .num_threads(num_workers)
        .build()
        .unwrap()
        .install(|| {
            (0..num_workers).into_par_iter().for_each(|thread_id| {
                let thread_start = Instant::now();
                let mut nonce = thread_id as u64; // every thread starts with a different nonce
                let mut local_attempts = 0u64;

                while !found.load(Ordering::Relaxed) {
                    let block = Block::new(previous_hash.clone(), timestamp, nonce, data.clone());
                    local_attempts += 1;

                    if block.meets_difficulty(difficulty) {
                        // check if it is the first to find it
                        if !found.swap(true, Ordering::SeqCst) {
                            let mining_result = MiningResult {
                                block,
                                attempts: local_attempts,
                                thread_id,
                            };
                            *result.lock() = Some(mining_result);
                        }
                        break;
                    }

                    nonce += num_workers as u64;
                }

                let thread_time = thread_start.elapsed().as_secs_f64();
                thread_attempts[thread_id].fetch_add(local_attempts, Ordering::Relaxed);
                *thread_times[thread_id].lock() = thread_time;
                global_attempts.fetch_add(local_attempts, Ordering::Relaxed);
            });
        });

    let mining_result = result.lock().take().expect("No mining result found");

    let thread_performance: Vec<ThreadPerformance> = (0..num_workers)
        .map(|i| ThreadPerformance {
            thread_id: i,
            blocks_found: if i == mining_result.thread_id { 1 } else { 0 },
            total_attempts: thread_attempts[i].load(Ordering::Relaxed),
            total_time_seconds: *thread_times[i].lock(),
        })
        .collect();

    let total_attempts = global_attempts.load(Ordering::Relaxed);

    (mining_result.block, total_attempts, thread_performance)
}

pub fn run_parallel_mining(
    num_blocks: usize,
    difficulty: usize,
    num_workers: usize,
    transactions_per_block: usize,
) -> Result<(), Box<dyn Error>> {
    println!("=== Parallel ProofOfWork Mining ===");
    println!("Blocks to mine: {}", num_blocks);
    println!("Difficulty: {}", difficulty);
    println!("Number of workers: {}", num_workers);
    println!("Transactions per block: {}", transactions_per_block);
    println!();

    let start_time = Instant::now();
    let mut blockchain = Blockchain::new(difficulty);
    let mut mining_progress = Vec::new();
    let mut total_attempts = 0u64;
    let mut all_thread_performance = Vec::new();

    for i in 1..=num_blocks {
        println!("Mining block {} with {} workers...", i, num_workers);
        let block_start = Instant::now();

        let transactions = generate_transactions(i, transactions_per_block);
        let block_data = create_block_data(i, &transactions);

        let previous_hash = blockchain.last_block().hash.clone();
        let timestamp = chrono::Utc::now().timestamp();

        let (block, attempts, thread_perf) = mine_block_parallel(
            previous_hash,
            timestamp,
            block_data,
            difficulty,
            num_workers,
        );
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

        for perf in thread_perf {
            all_thread_performance.push(perf);
        }

        let winner_thread = block.nonce % num_workers as u64;
        println!(
            "  Found by thread {}, Nonce: {} | Hash: {}... | Attempts: {} | Time: {:.3}s",
            winner_thread,
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

    println!("=== Thread Performance ===");
    let mut thread_stats: Vec<(usize, u64, usize)> = Vec::new();
    for thread_id in 0..num_workers {
        let thread_data: Vec<&ThreadPerformance> = all_thread_performance
            .iter()
            .filter(|p| p.thread_id == thread_id)
            .collect();

        let total_thread_attempts: u64 = thread_data.iter().map(|p| p.total_attempts).sum();
        let blocks_found: usize = thread_data.iter().map(|p| p.blocks_found).sum();

        thread_stats.push((thread_id, total_thread_attempts, blocks_found));
    }

    for (thread_id, attempts, blocks_found) in thread_stats {
        let percentage = (attempts as f64 / total_attempts as f64) * 100.0;
        println!(
            "Thread {}: {} attempts ({:.2}%), {} blocks found",
            thread_id, attempts, percentage, blocks_found
        );
    }
    println!();

    let config_suffix = get_config_suffix(
        difficulty,
        num_blocks,
        Some(transactions_per_block),
        Some(num_workers),
    );

    println!("Saving results...");
    let mining_filename = format!("pow_mining_parallel_rust_{}.json", config_suffix);
    let blockchain_filename = format!("pow_blockchain_parallel_rust_{}.json", config_suffix);
    let performance_filename = format!("pow_performance_parallel_rust_{}.csv", config_suffix);
    let thread_filename = format!("pow_thread_performance_rust_{}.json", config_suffix);

    save_mining_progress(&mining_progress, &mining_filename)?;
    save_blockchain(&blockchain, &blockchain_filename)?;
    save_thread_performance(&all_thread_performance, &thread_filename)?;

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
    println!("  - {}", thread_filename);

    Ok(())
}
