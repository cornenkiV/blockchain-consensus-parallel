use crate::blockchain::{Block, Blockchain};
use crate::p2p::error::NetworkError;
use crate::p2p::mempool::TransactionPool;
use crate::p2p::network::StarNetworkClient;
use crate::p2p::protocol::{BlockTemplate, P2PMessage, PeerInfo};
use crate::pos::Transaction;
use parking_lot::Mutex;
use std::io::{self, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

pub struct RegularNode {
    node_id: String,
    client: Arc<StarNetworkClient>,
    blockchain: Arc<Mutex<Blockchain>>,
    peers: Arc<Mutex<Vec<PeerInfo>>>,
    mempool: Arc<Mutex<TransactionPool>>,
    mining_active: Arc<AtomicBool>,
    mining_stop_flag: Arc<AtomicBool>,
    mining_thread: Arc<Mutex<Option<JoinHandle<()>>>>,
    running: Arc<AtomicBool>,
}

impl RegularNode {
    pub fn new(bootstrap_address: &str, node_id: String) -> Result<Self, NetworkError> {
        let client = StarNetworkClient::connect(bootstrap_address, node_id.clone())?;
        let blockchain = crate::blockchain::Blockchain {
            chain: Vec::new(),
            difficulty: 5,
        };
        let mempool = TransactionPool::new(1000);

        Ok(RegularNode {
            node_id,
            client: Arc::new(client),
            blockchain: Arc::new(Mutex::new(blockchain)),
            peers: Arc::new(Mutex::new(Vec::new())),
            mempool: Arc::new(Mutex::new(mempool)),
            mining_active: Arc::new(AtomicBool::new(false)),
            mining_stop_flag: Arc::new(AtomicBool::new(false)),
            mining_thread: Arc::new(Mutex::new(None)),
            running: Arc::new(AtomicBool::new(true)),
        })
    }

    ///connect to bootstrap
    pub fn connect(&mut self) -> Result<(), NetworkError> {
        println!("Connecting to bootstrap node...");

        let request = P2PMessage::RequestBlockchain {
            requester_id: self.node_id.clone(),
        };
        self.client.send(&request)?;
        println!("Blockchain sync requested");

        Ok(())
    }

    pub fn start(&mut self) -> Result<(), NetworkError> {
        self.start_heartbeat_thread();
        self.cli_loop();

        Ok(())
    }

    fn start_message_loop_thread(&self) {
        let client = self.client.clone();
        let blockchain = self.blockchain.clone();
        let peers = self.peers.clone();
        let mempool = self.mempool.clone();
        let mining_active = self.mining_active.clone();
        let mining_stop_flag = self.mining_stop_flag.clone();
        let mining_thread = self.mining_thread.clone();
        let running = self.running.clone();
        let node_id = self.node_id.clone();

        thread::spawn(move || {
            while running.load(Ordering::SeqCst) {
                match client.receive() {
                    Ok(message) => {
                        Self::handle_message_static(
                            &node_id,
                            message,
                            &blockchain,
                            &peers,
                            &mempool,
                            &client,
                            &mining_active,
                            &mining_stop_flag,
                            &mining_thread,
                            &running,
                        );
                    }
                    Err(e) => {
                        let error_str = e.to_string();

                        if error_str.contains("timed out")
                            || error_str.contains("WouldBlock")
                            || error_str.contains("10060")
                        {
                            // continue if timeout
                            continue;
                        }

                        //error connection lost
                        if running.load(Ordering::SeqCst) {
                            eprintln!("Connection lost: {}", e);
                            running.store(false, Ordering::SeqCst);
                        }
                        break;
                    }
                }
            }
        });
    }

    fn start_heartbeat_thread(&self) {
        let client = self.client.clone();
        let running = self.running.clone();
        let node_id = self.node_id.clone();

        thread::spawn(move || {
            let heartbeat = P2PMessage::Heartbeat {
                node_id: node_id.clone(),
                timestamp: chrono::Utc::now().timestamp() as u64,
            };

            if let Err(e) = client.send(&heartbeat) {
                eprintln!("Failed to send initial heartbeat: {}", e);
                running.store(false, Ordering::SeqCst);
                return;
            }

            //send heartbeat every 10s
            let mut count = 1;
            while running.load(Ordering::SeqCst) {
                thread::sleep(Duration::from_secs(10));

                if !running.load(Ordering::SeqCst) {
                    break;
                }

                count += 1;
                let heartbeat = P2PMessage::Heartbeat {
                    node_id: node_id.clone(),
                    timestamp: chrono::Utc::now().timestamp() as u64,
                };

                if let Err(e) = client.send(&heartbeat) {
                    eprintln!("Failed to send heartbeat #{}: {}", count, e);
                    running.store(false, Ordering::SeqCst);
                    break;
                }
            }
            println!("Heartbeat thread stopped");
        });
    }

    fn handle_message_static(
        node_id: &str,
        message: P2PMessage,
        blockchain: &Arc<Mutex<Blockchain>>,
        peers: &Arc<Mutex<Vec<PeerInfo>>>,
        mempool: &Arc<Mutex<TransactionPool>>,
        client: &Arc<StarNetworkClient>,
        mining_active: &Arc<AtomicBool>,
        mining_stop_flag: &Arc<AtomicBool>,
        mining_thread: &Arc<Mutex<Option<JoinHandle<()>>>>,
        running: &Arc<AtomicBool>,
    ) {
        match message {
            P2PMessage::PeerList { peers: peer_list } => {
                let mut peers_lock = peers.lock();
                *peers_lock = peer_list;
                println!("Peer list updated: {} peers", peers_lock.len());
            }

            P2PMessage::BlockchainSync { chain } => {
                println!("Received blockchain ({} blocks)", chain.len());

                let mut blockchain_lock = blockchain.lock();

                if blockchain_lock.chain.is_empty() {
                    if !crate::blockchain::Blockchain::validate_chain(
                        &chain,
                        blockchain_lock.difficulty,
                    ) {
                        eprintln!("   Received invalid chain, rejecting");
                        return;
                    }
                    blockchain_lock.chain = chain;
                    println!(
                        "   Blockchain initialized: {} blocks",
                        blockchain_lock.chain.len()
                    );
                    return;
                }

                if !blockchain_lock.is_longer_chain(&chain) {
                    println!("   Current chain is equal or longer, no action needed");
                    return;
                }

                if !crate::blockchain::Blockchain::validate_chain(
                    &chain,
                    blockchain_lock.difficulty,
                ) {
                    eprintln!("   Received invalid chain, rejecting");
                    return;
                }

                println!("   Received chain is longer and valid!");
                blockchain_lock.reorganize(chain);

                println!("   Chain reorganization complete");
                println!("   New chain length: {}", blockchain_lock.chain.len());
            }

            P2PMessage::NewBlock { block, miner_id } => {
                let is_valid = {
                    let blockchain_lock = blockchain.lock();
                    blockchain_lock.validate_block(&block)
                };

                if is_valid {
                    //adding to blockchain
                    {
                        let mut blockchain_lock = blockchain.lock();
                        blockchain_lock.add_block(block.clone());
                        println!("\nNew block added to chain (mined by {})", miner_id);
                        println!(
                            "  Block #{}: {}...",
                            blockchain_lock.chain.len() - 1,
                            &block.hash[..16]
                        );
                    }

                    mining_stop_flag.store(true, Ordering::SeqCst);
                    mining_active.store(false, Ordering::SeqCst);

                    {
                        let mut mempool_lock = mempool.lock();
                        mempool_lock.clear();
                    }
                } else {
                    println!("Received block doesn't fit current chain");

                    //request full blockchain to resolve fork
                    let msg = P2PMessage::RequestBlockchain {
                        requester_id: node_id.to_string(),
                    };
                    client.send(&msg).ok();
                }
            }

            P2PMessage::Pong { node_id: _ } => {
                //heartbeat response
            }

            P2PMessage::MiningStart { template } => {
                Self::handle_mining_start(
                    node_id,
                    template,
                    client,
                    mining_active,
                    mining_stop_flag,
                    mining_thread,
                );
            }

            P2PMessage::MiningStop => {
                println!("Mining stop signal received");
                mining_stop_flag.store(true, Ordering::SeqCst);
                mining_active.store(false, Ordering::SeqCst);
            }

            P2PMessage::NewTransaction {
                transaction,
                from_node,
            } => {
                let tx: Transaction = match serde_json::from_str(&transaction) {
                    Ok(tx) => tx,
                    Err(_) => return,
                };

                let mut mempool_lock = mempool.lock();
                if mempool_lock.add_transaction(tx.clone()).is_ok() {
                    println!("New transaction received (from {})", from_node);
                    println!("   {} -> {}: {} coins", tx.from, tx.to, tx.amount);
                }
            }

            _ => {
                eprintln!("Unexpected message type: {:?}", message);
            }
        }
    }

    fn cli_loop(&self) {
        loop {
            if !self.running.load(Ordering::SeqCst) {
                println!("\nConnection lost. Exiting...");
                break;
            }

            print!("{}> ", self.node_id);
            io::stdout().flush().ok();

            let mut input = String::new();
            if io::stdin().read_line(&mut input).is_err() {
                break;
            }

            match input.trim() {
                "blockchain" => self.show_blockchain(),
                "peers" => self.show_peers(),
                "status" => self.show_status(),
                "sync" => self.force_sync(),
                "add-tx" => self.add_transaction_interactive(),
                "mempool" => self.show_mempool(),
                "mining-status" => self.show_mining_status(),
                "simulate-fork" => self.simulate_fork(),
                "help" => self.show_help(),
                "exit" | "quit" => {
                    println!("Shutting down node...");
                    break;
                }
                "" => {}
                _ => println!("Unknown command. Type 'help' for commands."),
            }
        }

        self.running.store(false, Ordering::SeqCst);
    }

    fn show_blockchain(&self) {
        let blockchain = self.blockchain.lock();
        println!("\n=== Blockchain ===");
        println!("Length: {} blocks", blockchain.chain.len());
        println!("Difficulty: {}", blockchain.difficulty);
        println!();

        for (i, block) in blockchain.chain.iter().enumerate() {
            if i == 0 {
                println!("#{}: Genesis Block", i);
                println!("    Hash: {}...", &block.hash[..32.min(block.hash.len())]);
            } else {
                println!("#{}: Block {}", i, i);
                println!("    Hash: {}...", &block.hash[..32.min(block.hash.len())]);
                println!(
                    "    Previous: {}...",
                    &block.previous_hash[..32.min(block.previous_hash.len())]
                );
                println!("    Timestamp: {}", block.timestamp);
                println!("    Nonce: {}", block.nonce);
            }
        }
        println!();
    }

    fn show_peers(&self) {
        let peers = self.peers.lock();
        println!("\n=== Connected Peers ===");

        if peers.is_empty() {
            println!("No other peers connected");
        } else {
            for peer in peers.iter() {
                let last_seen_ago = chrono::Utc::now().timestamp() as u64 - peer.last_seen;
                println!(
                    "- {} ({}) - last seen {}s ago",
                    peer.node_id, peer.address, last_seen_ago
                );
            }
        }
        println!();
    }

    fn show_status(&self) {
        let blockchain = self.blockchain.lock();
        let peers = self.peers.lock();

        println!("\n=== Node Status ===");
        println!("Node ID: {}", self.node_id);
        println!("Bootstrap: {}", self.client.bootstrap_address());
        println!("Blockchain: {} blocks", blockchain.chain.len());
        println!("Peers: {}", peers.len());

        if self.running.load(Ordering::SeqCst) {
            println!("Status: Connected");
        } else {
            println!("Status: Disconnected");
        }
        println!();
    }

    fn force_sync(&self) {
        println!("Requesting blockchain sync...");
        let request = P2PMessage::RequestBlockchain {
            requester_id: self.node_id.clone(),
        };

        if let Err(e) = self.client.send(&request) {
            eprintln!("Failed to request sync: {}", e);
        } else {
            println!("Sync request sent");
        }
    }

    fn simulate_fork(&self) {
        println!("\n=== Fork Simulation ===");
        println!("This simulates a scenario where this node has a shorter chain");
        println!();

        println!("1: Getting current blockchain from network...");
        self.force_sync();
        thread::sleep(Duration::from_millis(500));

        {
            let mut blockchain = self.blockchain.lock();
            if blockchain.chain.len() > 1 {
                blockchain.chain.pop();
                println!("2: Removed last block from chain");
                println!("   Local chain now: {} blocks", blockchain.chain.len());
            } else {
                println!("   Chain too short for fork simulation");
                return;
            }
        }

        println!("\n3: Requesting sync from network...");
        println!("Network should have longer chain, fork will be resolved");
        thread::sleep(Duration::from_secs(1));
        self.force_sync();
    }

    fn show_help(&self) {
        println!("\n=== Available Commands ===");
        println!("  blockchain     - Show blockchain");
        println!("  peers          - Show connected peers");
        println!("  status         - Show node status");
        println!("  sync           - Blockchain sync");
        println!("  add-tx         - Add new transaction");
        println!("  mempool        - Show pending transactions");
        println!("  mining-status  - Show mining status");
        println!("  simulate-fork  - Simulate fork scenario");
        println!("  help           - Show all commands");
        println!("  exit           - Shutdown node");
        println!();
    }

    fn add_transaction_interactive(&self) {
        print!("From: ");
        io::stdout().flush().ok();
        let mut from = String::new();
        io::stdin().read_line(&mut from).ok();

        print!("To: ");
        io::stdout().flush().ok();
        let mut to = String::new();
        io::stdin().read_line(&mut to).ok();

        print!("Amount: ");
        io::stdout().flush().ok();
        let mut amount_str = String::new();
        io::stdin().read_line(&mut amount_str).ok();

        let amount: u64 = match amount_str.trim().parse() {
            Ok(a) => a,
            Err(_) => {
                println!("Invalid amount");
                return;
            }
        };

        let tx = Transaction::new(from.trim().to_string(), to.trim().to_string(), amount);

        let mut mempool = self.mempool.lock();
        if let Err(e) = mempool.add_transaction(tx.clone()) {
            println!("Failed: {}", e);
            return;
        }
        drop(mempool);

        let tx_json = serde_json::to_string(&tx).unwrap();
        let msg = P2PMessage::NewTransaction {
            transaction: tx_json,
            from_node: self.node_id.clone(),
        };

        if let Err(e) = self.client.send(&msg) {
            println!("Failed to broadcast: {}", e);
        } else {
            println!("Transaction added and broadcast to network");
        }
    }

    fn show_mempool(&self) {
        let mempool = self.mempool.lock();
        println!("\n=== Local Mempool ===");
        println!("Pending transactions: {}", mempool.size());

        if mempool.size() == 0 {
            println!("(empty)");
        } else {
            for (i, tx) in mempool.get_all().iter().enumerate() {
                println!("{}. {} -> {}: {} coins", i + 1, tx.from, tx.to, tx.amount);
            }
        }
        println!();
    }

    fn handle_mining_start(
        node_id: &str,
        template: BlockTemplate,
        client: &Arc<StarNetworkClient>,
        mining_active: &Arc<AtomicBool>,
        mining_stop_flag: &Arc<AtomicBool>,
        mining_thread: &Arc<Mutex<Option<JoinHandle<()>>>>,
    ) {
        println!("\nMining start signal received");
        println!(
            "  Block #{}, Difficulty: {}",
            template.block_number, template.difficulty
        );

        mining_stop_flag.store(true, Ordering::SeqCst);

        if let Some(handle) = mining_thread.lock().take() {
            handle.join().ok();
        }

        mining_stop_flag.store(false, Ordering::SeqCst);
        mining_active.store(true, Ordering::SeqCst);

        let node_id = node_id.to_string();
        let client = client.clone();
        let stop_flag = Arc::clone(mining_stop_flag);
        let mining_active_clone = Arc::clone(mining_active);

        let handle = thread::spawn(move || {
            println!("Mining started...");
            let start_time = std::time::Instant::now();

            let result = crate::p2p::mining::mine_block_parallel(
                template,
                4, //num_workers
                stop_flag.clone(),
            );

            if let Some(block) = result {
                let elapsed = start_time.elapsed().as_secs_f64();
                println!("\nBLOCK FOUND!");
                println!("  Nonce: {}", block.nonce);
                println!("  Hash: {}...", &block.hash[..16]);
                println!("  Time: {:.2}s", elapsed);

                let msg = P2PMessage::NewBlock {
                    block,
                    miner_id: node_id,
                };

                client.send(&msg).ok();
            } else {
                println!("Mining stopped (block found by another node)");
            }

            mining_active_clone.store(false, Ordering::SeqCst);
        });

        *mining_thread.lock() = Some(handle);
    }

    fn show_mining_status(&self) {
        println!("\n=== Mining Status ===");

        if self.mining_active.load(Ordering::SeqCst) {
            println!("Status: MINING");
        } else {
            println!("Status: IDLE");
        }

        let blockchain = self.blockchain.lock();
        println!("Current chain length: {}", blockchain.chain.len());
        println!();
    }
}

pub fn run_regular_node(
    bootstrap_address: &str,
    node_id: Option<String>,
) -> Result<(), NetworkError> {
    let node_id = node_id.unwrap_or_else(|| format!("node_{}", rand::random::<u16>()));

    println!("================================================");
    println!("Regular Node Starting");
    println!("================================================");
    println!("Node ID: {}", node_id);
    println!("Bootstrap: {}", bootstrap_address);
    println!();

    let mut node = RegularNode::new(bootstrap_address, node_id)?;

    node.connect()?;

    node.start_message_loop_thread();

    println!("Waiting for initial sync...");
    thread::sleep(Duration::from_millis(500));

    println!("\nNode ready!");

    node.start()?;

    Ok(())
}
