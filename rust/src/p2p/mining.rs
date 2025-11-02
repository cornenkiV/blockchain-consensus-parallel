use crate::blockchain::Block;
use crate::p2p::protocol::BlockTemplate;
use parking_lot::Mutex;
use rayon::prelude::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

#[derive(Debug, Clone)]
struct MiningResult {
    block: Block,
    #[allow(dead_code)]
    attempts: u64,
    #[allow(dead_code)]
    thread_id: usize,
}

pub fn mine_block_parallel(
    template: BlockTemplate,
    num_workers: usize,
    stop_flag: Arc<AtomicBool>,
) -> Option<Block> {
    let found = Arc::new(AtomicBool::new(false));
    let global_attempts = Arc::new(AtomicU64::new(0));

    let result: Arc<Mutex<Option<MiningResult>>> = Arc::new(Mutex::new(None));

    let thread_attempts: Vec<Arc<AtomicU64>> = (0..num_workers)
        .map(|_| Arc::new(AtomicU64::new(0)))
        .collect();

    let data = format!(
        "Block {} with {} transactions",
        template.block_number,
        template.transactions.len()
    );

    let previous_hash = template.previous_hash.clone();
    let timestamp = template.timestamp;
    let difficulty = template.difficulty;

    rayon::ThreadPoolBuilder::new()
        .num_threads(num_workers)
        .build()
        .unwrap()
        .install(|| {
            (0..num_workers).into_par_iter().for_each(|thread_id| {
                let mut nonce = thread_id as u64;
                let mut local_attempts = 0u64;

                while !found.load(Ordering::Relaxed) && !stop_flag.load(Ordering::Relaxed) {
                    let block = Block::new(previous_hash.clone(), timestamp, nonce, data.clone());
                    local_attempts += 1;

                    if block.meets_difficulty(difficulty) {
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

                thread_attempts[thread_id].fetch_add(local_attempts, Ordering::Relaxed);
                global_attempts.fetch_add(local_attempts, Ordering::Relaxed);
            });
        });

    if stop_flag.load(Ordering::Relaxed) && !found.load(Ordering::Relaxed) {
        return None;
    }

    result.lock().take().map(|r| r.block)
}
