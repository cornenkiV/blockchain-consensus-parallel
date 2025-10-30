mod blockchain;
mod p2p;
mod pos;
mod pow_parallel;
mod pow_sequential;
mod utils;

use clap::{Parser, Subcommand, ValueEnum};
use std::error::Error;
use std::process;

#[derive(Debug, Clone, ValueEnum)]
enum ConsensusMode {
    PowSequential,
    PowParallel,
    Pos,
}

#[derive(Parser, Debug)]
#[command(name = "blockchain-consensus")]
#[command(about = "Blockchain consensus PoW and PoS")]
struct Args {
    #[command(subcommand)]
    command: Option<Command>,

    #[arg(short, long, value_enum, default_value = "pow-sequential")]
    mode: ConsensusMode,

    #[arg(short, long, default_value = "8")]
    workers: usize,

    #[arg(short, long, default_value = "4")]
    difficulty: usize,

    #[arg(short, long, default_value = "20")]
    blocks: usize,

    #[arg(short, long, default_value = "5")]
    transactions: usize,
}

#[derive(Subcommand, Debug)]
enum Command {
    Bootstrap {
        #[arg(short, long, default_value = "8000")]
        port: u16,
    },
    Node {
        #[arg(short, long)]
        connect: String,

        #[arg(long)]
        node_id: Option<String>,
    },
}

fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {}", e);
        process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    if let Some(command) = args.command {
        return match command {
            Command::Bootstrap { port } => p2p::run_bootstrap_node(port),
            Command::Node { connect, node_id } => {
                p2p::run_regular_node(&connect, node_id).map_err(|e| e.into())
            }
        };
    }

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
        ConsensusMode::PowParallel => println!("  Workers: {}", args.workers),
        ConsensusMode::Pos => println!("  Validators: {}", args.workers),
        ConsensusMode::PowSequential => {}
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
        ConsensusMode::PowSequential => {
            pow_sequential::run_sequential_mining(args.blocks, args.difficulty, args.transactions)
        }
        ConsensusMode::PowParallel => {
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
        ConsensusMode::Pos => {
            if args.workers < 2 {
                return Err("PoS requires at least 2 validators".into());
            }
            pos::run_pos_consensus(args.workers, args.blocks, args.transactions)
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
