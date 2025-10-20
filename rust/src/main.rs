mod blockchain;
mod pow_parallel;
mod pow_sequential;
mod utils;

use clap::{Parser, ValueEnum};
use std::error::Error;
use std::process;

#[derive(Debug, Clone, ValueEnum)]
enum MiningMode {
    Sequential,
    Parallel,
}

#[derive(Parser, Debug)]
struct Args {
    #[arg(short, long, value_enum, default_value = "sequential")]
    mode: MiningMode,

    #[arg(short, long, default_value = "4")]
    workers: usize,

    #[arg(short, long, default_value = "4")]
    difficulty: usize,

    #[arg(short, long, default_value = "20")]
    blocks: usize,

    #[arg(short, long, default_value = "5")]
    transactions: usize,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {}", e);
        process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    if args.blocks == 0 {
        return Err("Number of blocks must be at least 1".into());
    }

    println!("===========================================================");
    println!("Blockchain ProofOfWork mining");
    println!("===========================================================");
    println!();
    println!("Configuration:");
    println!("  Mode: {:?}", args.mode);
    match args.mode {
        MiningMode::Parallel => println!("  Workers: {}", args.workers),
        MiningMode::Sequential => {}
    }
    println!(
        "  Difficulty: {} (target: {}...)",
        args.difficulty,
        "0".repeat(args.difficulty)
    );
    println!("  Blocks to mine: {}", args.blocks);
    println!("  Transactions per block: {}", args.transactions);
    println!();

    let result = match args.mode {
        MiningMode::Sequential => {
            pow_sequential::run_sequential_mining(args.blocks, args.difficulty, args.transactions)
        }
        MiningMode::Parallel => {
            if args.workers < 1 {
                return Err("Number of workers must be at least 1".into());
            }
            pow_parallel::run_parallel_mining(
                args.blocks,
                args.difficulty,
                args.workers,
                args.transactions,
            )
        }
    };

    match result {
        Ok(_) => {
            println!();
            println!("===========================================================");
            println!("Mining completed");
            println!("===========================================================");
            Ok(())
        }
        Err(e) => {
            eprintln!();
            println!("===========================================================");
            eprintln!("Mining failed");
            println!("===========================================================");
            Err(e)
        }
    }
}
