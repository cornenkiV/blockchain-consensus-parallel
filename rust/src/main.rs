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
    P2pServer {
        #[arg(short, long, default_value = "8000")]
        port: u16,
    },
    P2pClient {
        #[arg(short, long)]
        connect: String,

        #[arg(short, long, default_value = "test_client")]
        node_id: String,
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
            Command::P2pServer { port } => run_p2p_server(port),
            Command::P2pClient { connect, node_id } => run_p2p_client(&connect, node_id),
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

fn run_p2p_server(port: u16) -> Result<(), Box<dyn Error>> {
    use p2p::{NetworkLayer, P2PMessage, PeerInfo, StarNetworkServer};

    println!("===========================================================");
    println!("Network Server (Bootstrap Node)");
    println!("===========================================================");
    println!("Listening on: 0.0.0.0:{}", port);
    println!();

    let server = StarNetworkServer::new(port)?;

    println!("Waiting for peer connections...");
    println!("(Press Ctrl+C to stop)");
    println!();

    loop {
        match server.accept_connection() {
            Ok((node_id, stream)) => {
                server.register_peer(node_id.clone(), stream);
                println!("Active peers: {}", server.peer_count());

                let peers = server
                    .get_connected_peers()
                    .into_iter()
                    .filter(|id| id != &node_id)
                    .map(|id| PeerInfo {
                        node_id: id,
                        address: "unknown".to_string(),
                        last_seen: chrono::Utc::now().timestamp() as u64,
                        blocks_mined: 0,
                    })
                    .collect();

                let peer_list_msg = P2PMessage::PeerList { peers };

                if let Err(e) = server.send_to(&node_id, &peer_list_msg) {
                    eprintln!("Failed to send PeerList to {}: {}", node_id, e);
                }

                println!();
            }
            Err(e) => {
                eprintln!("Connection error: {}", e);
            }
        }
    }
}

fn run_p2p_client(bootstrap_address: &str, node_id: String) -> Result<(), Box<dyn Error>> {
    use p2p::{P2PMessage, StarNetworkClient};
    use std::io::{self, Write};

    println!("===========================================================");
    println!("Network Client (Regular Node)");
    println!("===========================================================");
    println!("Node ID: {}", node_id);
    println!("Connecting to bootstrap: {}", bootstrap_address);
    println!();

    let client = StarNetworkClient::connect(bootstrap_address, node_id.clone())?;
    println!("Connected to bootstrap");
    println!();

    match client.receive() {
        Ok(P2PMessage::PeerList { peers }) => {
            println!("Received peer list from bootstrap:");
            if peers.is_empty() {
                println!("  (no other peers currently connected)");
            } else {
                for peer in peers {
                    println!("  - {}", peer.node_id);
                }
            }
            println!();
        }
        Ok(msg) => {
            println!("Received unexpected message: {:?}", msg);
            println!();
        }
        Err(e) => {
            eprintln!("Failed to receive PeerList: {}", e);
        }
    }

    println!("Commands:");
    println!("  heartbeat - Send heartbeat to bootstrap");
    println!("  ping - Send test message");
    println!("  quit - Disconnect and exit");
    println!();

    loop {
        print!("> ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let command = input.trim();

        match command {
            "heartbeat" => {
                let msg = P2PMessage::Heartbeat {
                    node_id: node_id.clone(),
                    timestamp: chrono::Utc::now().timestamp() as u64,
                };
                match client.send(&msg) {
                    Ok(_) => println!("Heartbeat sent"),
                    Err(e) => eprintln!("Failed to send: {}", e),
                }
            }
            "ping" => {
                let msg = P2PMessage::NewTransaction {
                    transaction: format!("Test from {}", node_id),
                    from_node: node_id.clone(),
                };
                match client.send(&msg) {
                    Ok(_) => println!("Ping sent"),
                    Err(e) => eprintln!("Failed to send: {}", e),
                }
            }
            "quit" => {
                println!("Disconnecting...");
                break;
            }
            "" => continue,
            _ => {
                println!("Unknown command: {}", command);
            }
        }
    }

    Ok(())
}
