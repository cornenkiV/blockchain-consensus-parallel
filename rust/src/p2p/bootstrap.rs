use crate::blockchain::Blockchain;
use crate::p2p::error::NetworkError;
use crate::p2p::logger::{NetworkEvent, NetworkLogger};
use crate::p2p::mempool::TransactionPool;
use crate::p2p::metrics::NetworkStatistics;
use crate::p2p::network::{NetworkLayer, StarNetworkServer};
use crate::p2p::protocol::{BlockTemplate, P2PMessage, PeerInfo};
use crate::pos::Transaction;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::io::{self, Write};
use std::net::TcpStream;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

/// coordinator for star topology P2P network
pub struct BootstrapNode {
    network: Arc<StarNetworkServer>,
    blockchain: Arc<Mutex<Blockchain>>,
    peers: Arc<Mutex<HashMap<String, PeerInfo>>>,
    mempool: Arc<Mutex<TransactionPool>>,
    mining_active: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
    start_time: Instant,
    logger: NetworkLogger,
    mining_round: Arc<AtomicU64>,
    mining_round_start: Arc<Mutex<Option<Instant>>>,
}

impl BootstrapNode {
    pub fn new(port: u16) -> Result<Self, NetworkError> {
        let network = StarNetworkServer::new(port)?;
        let blockchain = Blockchain::new(5);
        let mempool = TransactionPool::new(1000);

        Ok(BootstrapNode {
            network: Arc::new(network),
            blockchain: Arc::new(Mutex::new(blockchain)),
            peers: Arc::new(Mutex::new(HashMap::new())),
            mempool: Arc::new(Mutex::new(mempool)),
            mining_active: Arc::new(AtomicBool::new(false)),
            running: Arc::new(AtomicBool::new(true)),
            start_time: Instant::now(),
            logger: NetworkLogger::new(),
            mining_round: Arc::new(AtomicU64::new(0)),
            mining_round_start: Arc::new(Mutex::new(None)),
        })
    }

    /// starts multiple threads:
    /// - Connection handler thread
    /// - Heartbeat monitor thread
    /// - CLI command loop
    pub fn start(&mut self) -> Result<(), NetworkError> {
        self.start_connection_handler();

        self.start_heartbeat_monitor();

        println!("Commands:");
        println!("  peers        - Show connected peers");
        println!("  blockchain   - Show blockchain summary");
        println!("  mempool      - Show pending transactions");
        println!("  clear-mempool- Clear all pending transactions");
        println!("  start-mining - Initiate mining");
        println!("  stop-mining  - Stop current mining");
        println!("  sync         - Request blockchain from peers");
        println!("  stats        - Show node statistics");
        println!("  network-stats- Show network statistics from events");
        println!("  export-logs  - Export logs to JSON/CSV");
        println!("  clear-logs   - Clear all logged events");
        println!("  quit         - Shutdown bootstrap node");
        println!();

        self.command_loop()?;
        self.shutdown();

        Ok(())
    }

    /// cli loop
    fn command_loop(&self) -> Result<(), NetworkError> {
        let stdin = io::stdin();

        loop {
            print!("> ");
            io::stdout().flush().ok();

            let mut input = String::new();
            if stdin.read_line(&mut input).is_err() {
                break;
            }

            match input.trim() {
                "peers" => self.show_peers(),
                "blockchain" => self.show_blockchain(),
                "mempool" => self.show_mempool(),
                "clear-mempool" => self.clear_mempool(),
                "start-mining" => self.initiate_mining_round(),
                "stop-mining" => self.stop_mining(),
                "sync" => self.request_blockchain_sync(),
                "stats" => self.show_stats(),
                "network-stats" => self.show_network_stats(),
                "export-logs" => self.export_logs(),
                "clear-logs" => self.clear_logs(),
                "quit" => {
                    println!("Shutting down bootstrap node...");
                    break;
                }
                "" => continue,
                cmd => println!("Unknown command: {}", cmd),
            }
        }

        Ok(())
    }

    fn start_connection_handler(&self) {
        let network = self.network.clone();
        let blockchain = self.blockchain.clone();
        let peers = self.peers.clone();
        let mempool = self.mempool.clone();
        let mining_active = self.mining_active.clone();
        let running = self.running.clone();
        let logger = self.logger.clone();
        let mining_round = self.mining_round.clone();
        let mining_round_start = self.mining_round_start.clone();

        thread::spawn(move || {
            while running.load(Ordering::Relaxed) {
                match network.accept_connection() {
                    Ok((node_id, stream)) => {
                        network.register_peer(node_id.clone(), stream.try_clone().unwrap());

                        let timestamp = chrono::Utc::now().timestamp() as u64;
                        {
                            let mut peers_lock = peers.lock();
                            peers_lock.insert(
                                node_id.clone(),
                                PeerInfo {
                                    node_id: node_id.clone(),
                                    address: "test".to_string(),
                                    last_seen: timestamp,
                                    blocks_mined: 0,
                                },
                            );
                        }

                        logger.log(NetworkEvent::PeerConnected {
                            node_id: node_id.clone(),
                            address: "test".to_string(),
                            timestamp,
                        });

                        let peer_list = Self::get_peer_list_static(&peers);
                        let peer_list_msg = P2PMessage::PeerList {
                            peers: peer_list
                                .into_iter()
                                .filter(|p| p.node_id != node_id)
                                .collect(),
                        };

                        if let Err(e) = network.send_to(&node_id, &peer_list_msg) {
                            eprintln!("Failed to send PeerList to {}: {}", node_id, e);
                        }

                        let all_peers = Self::get_peer_list_static(&peers);
                        let broadcast_msg = P2PMessage::PeerList { peers: all_peers };
                        network.broadcast(&broadcast_msg).ok();

                        Self::start_peer_message_loop(
                            node_id,
                            stream,
                            network.clone(),
                            blockchain.clone(),
                            peers.clone(),
                            mempool.clone(),
                            mining_active.clone(),
                            running.clone(),
                            logger.clone(),
                            mining_round.clone(),
                            mining_round_start.clone(),
                        );
                    }
                    Err(e) => {
                        if running.load(Ordering::Relaxed) {
                            eprintln!("Connection error: {}", e);
                            thread::sleep(Duration::from_millis(100));
                        }
                    }
                }
            }
        });
    }

    fn start_peer_message_loop(
        node_id: String,
        mut stream: TcpStream,
        network: Arc<StarNetworkServer>,
        blockchain: Arc<Mutex<Blockchain>>,
        peers: Arc<Mutex<HashMap<String, PeerInfo>>>,
        mempool: Arc<Mutex<TransactionPool>>,
        mining_active: Arc<AtomicBool>,
        running: Arc<AtomicBool>,
        logger: NetworkLogger,
        mining_round: Arc<AtomicU64>,
        mining_round_start: Arc<Mutex<Option<Instant>>>,
    ) {
        thread::spawn(move || {
            while running.load(Ordering::Relaxed) {
                match P2PMessage::receive(&mut stream) {
                    Ok(message) => {
                        Self::handle_message_static(
                            &node_id,
                            message,
                            &mut stream,
                            &network,
                            &blockchain,
                            &peers,
                            &mempool,
                            &mining_active,
                            &logger,
                            &mining_round,
                            &mining_round_start,
                        );
                    }
                    Err(e) => {
                        eprintln!("Error receiving from {}: {}", node_id, e);
                        // peer disconnected
                        let timestamp = chrono::Utc::now().timestamp() as u64;
                        logger.log(NetworkEvent::PeerDisconnected {
                            node_id: node_id.clone(),
                            timestamp,
                        });
                        Self::remove_peer_static(&node_id, &network, &peers);
                        break;
                    }
                }
            }
        });
    }

    /// handle incoming message from peer
    fn handle_message_static(
        from_node: &str,
        message: P2PMessage,
        stream: &mut TcpStream,
        network: &Arc<StarNetworkServer>,
        blockchain: &Arc<Mutex<Blockchain>>,
        peers: &Arc<Mutex<HashMap<String, PeerInfo>>>,
        mempool: &Arc<Mutex<TransactionPool>>,
        mining_active: &Arc<AtomicBool>,
        logger: &NetworkLogger,
        mining_round: &Arc<AtomicU64>,
        mining_round_start: &Arc<Mutex<Option<Instant>>>,
    ) {
        match message {
            P2PMessage::RequestBlockchain { requester_id } => {
                println!("{} requested blockchain", requester_id);

                logger.log(NetworkEvent::ChainSyncRequested {
                    requesting_node: requester_id.clone(),
                    timestamp: chrono::Utc::now().timestamp() as u64,
                });

                let chain = {
                    let blockchain_lock = blockchain.lock();
                    blockchain_lock.chain.clone()
                };

                let chain_len = chain.len();
                let sync_msg = P2PMessage::BlockchainSync { chain };
                if let Err(e) = sync_msg.send(stream) {
                    eprintln!("Failed to send blockchain to {}: {}", requester_id, e);
                } else {
                    println!("Sent blockchain to {} ({} blocks)", requester_id, chain_len);
                }
            }

            P2PMessage::BlockchainSync { chain } => {
                println!(
                    "Received blockchain from {} ({} blocks)",
                    from_node,
                    chain.len()
                );

                logger.log(NetworkEvent::ChainSyncReceived {
                    chain_length: chain.len(),
                    timestamp: chrono::Utc::now().timestamp() as u64,
                });

                let mut blockchain_lock = blockchain.lock();
                let old_height = blockchain_lock.chain.len();

                if !blockchain_lock.is_longer_chain(&chain) {
                    println!("   Chain is not longer, ignoring");
                    return;
                }

                if !crate::blockchain::Blockchain::validate_chain(
                    &chain,
                    blockchain_lock.difficulty,
                ) {
                    eprintln!("   Chain validation failed, rejecting");
                    return;
                }

                println!("   Chain is valid and longer - accepting!");

                logger.log(NetworkEvent::ChainReorganization {
                    old_height,
                    new_height: chain.len(),
                    timestamp: chrono::Utc::now().timestamp() as u64,
                });

                blockchain_lock.reorganize(chain.clone());
                drop(blockchain_lock);

                let msg = P2PMessage::BlockchainSync { chain };
                network.broadcast(&msg).ok();
            }

            P2PMessage::Heartbeat { node_id, timestamp } => {
                {
                    let mut peers_lock = peers.lock();
                    if let Some(peer) = peers_lock.get_mut(&node_id) {
                        peer.last_seen = timestamp;
                    }
                }

                let pong_msg = P2PMessage::Pong {
                    node_id: "bootstrap".to_string(),
                };
                if let Err(e) = pong_msg.send(stream) {
                    eprintln!("Failed to send Pong to {}: {}", node_id, e);
                }
            }

            P2PMessage::NewTransaction {
                transaction,
                from_node,
            } => {
                let tx: Transaction = match serde_json::from_str(&transaction) {
                    Ok(tx) => tx,
                    Err(e) => {
                        eprintln!("Invalid transaction format from {}: {}", from_node, e);
                        return;
                    }
                };

                let tx_hash = format!("{:x}", md5::compute(transaction.as_bytes()));
                let timestamp = chrono::Utc::now().timestamp() as u64;

                logger.log(NetworkEvent::TransactionReceived {
                    tx_hash: tx_hash.clone(),
                    timestamp,
                });

                //save it to mempool
                let mut mempool_lock = mempool.lock();
                if let Err(e) = mempool_lock.add_transaction(tx.clone()) {
                    eprintln!("Failed to add transaction from {}: {}", from_node, e);
                    return;
                }

                logger.log(NetworkEvent::TransactionAdded {
                    tx_hash: tx_hash.clone(),
                    timestamp,
                });

                println!("New transaction added to mempool (from {})", from_node);
                println!("   {} -> {}: {} coins", tx.from, tx.to, tx.amount);
                println!("   Mempool size: {}", mempool_lock.size());
                drop(mempool_lock);

                logger.log(NetworkEvent::TransactionBroadcast { tx_hash, timestamp });

                let relay_msg = P2PMessage::NewTransaction {
                    transaction,
                    from_node,
                };
                network.broadcast(&relay_msg).ok();
            }

            P2PMessage::Pong { node_id } => {
                println!("Pong from {}", node_id);
            }

            P2PMessage::NewBlock { block, miner_id } => {
                println!("\nNew block found by {}!", miner_id);

                let timestamp = chrono::Utc::now().timestamp() as u64;
                let block_height = {
                    let blockchain_lock = blockchain.lock();
                    blockchain_lock.chain.len()
                };

                logger.log(NetworkEvent::BlockReceived {
                    block_height,
                    block_hash: block.hash.clone(),
                    miner_id: miner_id.clone(),
                    timestamp,
                });

                let is_valid = {
                    let blockchain_lock = blockchain.lock();
                    blockchain_lock.validate_block(&block)
                };

                if !is_valid {
                    eprintln!("Invalid block, rejected");
                    logger.log(NetworkEvent::BlockRejected {
                        block_hash: block.hash.clone(),
                        reason: "validation_failed".to_string(),
                        timestamp,
                    });
                    return;
                }

                //adding to blockchain
                {
                    let mut blockchain_lock = blockchain.lock();
                    blockchain_lock.add_block(block.clone());
                    let new_height = blockchain_lock.chain.len() - 1;
                    println!("Block #{} added to blockchain", new_height);
                    println!("  Hash: {}...", &block.hash[..16]);
                    println!("  Nonce: {}", block.nonce);

                    logger.log(NetworkEvent::BlockAdded {
                        block_height: new_height,
                        block_hash: block.hash.clone(),
                        timestamp,
                    });
                }

                mining_active.store(false, Ordering::SeqCst);

                let round_num = mining_round.load(Ordering::SeqCst);
                if let Some(start_time) = mining_round_start.lock().take() {
                    let duration_ms = start_time.elapsed().as_millis() as u64;
                    logger.log(NetworkEvent::MiningRoundCompleted {
                        round_number: round_num,
                        winner_id: miner_id.clone(),
                        duration_ms,
                        timestamp,
                    });
                }

                logger.log(NetworkEvent::BlockBroadcast {
                    block_height,
                    block_hash: block.hash.clone(),
                    timestamp,
                });

                let msg = P2PMessage::NewBlock {
                    block: block.clone(),
                    miner_id: miner_id.clone(),
                };
                network.broadcast(&msg).ok();

                network.broadcast(&P2PMessage::MiningStop).ok();

                //delete mined tx from mempool
                {
                    let mut mempool_lock = mempool.lock();
                    let cleared = mempool_lock.size();
                    mempool_lock.clear();
                    println!("Cleared {} transactions from mempool", cleared);
                }

                println!("Mining complete.\n");
            }

            _ => {
                eprintln!("Unhandled message type from {}: {:?}", from_node, message);
            }
        }
    }

    /// heartbeat monitor thread
    fn start_heartbeat_monitor(&self) {
        let peers = self.peers.clone();
        let running = self.running.clone();

        thread::spawn(move || {
            while running.load(Ordering::Relaxed) {
                thread::sleep(Duration::from_secs(30));

                let now = chrono::Utc::now().timestamp() as u64;
                let mut peers_lock = peers.lock();
                let mut disconnected = Vec::new();

                for (node_id, peer_info) in peers_lock.iter() {
                    if now - peer_info.last_seen > 60 {
                        disconnected.push(node_id.clone());
                    }
                }

                for node_id in disconnected {
                    println!("Peer timeout: {} (no heartbeat for 60s)", node_id);
                    peers_lock.remove(&node_id);
                }
            }
        });
    }

    fn remove_peer_static(
        node_id: &str,
        network: &Arc<StarNetworkServer>,
        peers: &Arc<Mutex<HashMap<String, PeerInfo>>>,
    ) {
        {
            let mut peers_lock = peers.lock();
            peers_lock.remove(node_id);
        }

        let peer_list = Self::get_peer_list_static(peers);
        let msg = P2PMessage::PeerList { peers: peer_list };
        network.broadcast(&msg).ok();
    }

    fn get_peer_list_static(peers: &Arc<Mutex<HashMap<String, PeerInfo>>>) -> Vec<PeerInfo> {
        let peers_lock = peers.lock();
        peers_lock.values().cloned().collect()
    }

    fn shutdown(&self) {
        println!("Stopping bootstrap node...");
        self.running.store(false, Ordering::Relaxed);
        println!("All threads signaled to stop");
    }

    fn show_peers(&self) {
        let peers = self.peers.lock();

        if peers.is_empty() {
            println!("No peers connected");
            return;
        }

        println!("Connected Peers ({}):", peers.len());
        println!("┌──────────────────┬─────────────────────┬────────────┐");
        println!("│ Node ID          │ Last Seen           │ Blocks     │");
        println!("├──────────────────┼─────────────────────┼────────────┤");

        for peer in peers.values() {
            let last_seen_ago = chrono::Utc::now().timestamp() as u64 - peer.last_seen;
            println!(
                "│ {:<16} │ {}s ago{:<11} │ {:>10} │",
                &peer.node_id[..peer.node_id.len().min(16)],
                last_seen_ago,
                "",
                peer.blocks_mined
            );
        }

        println!("└──────────────────┴─────────────────────┴────────────┘");
    }

    fn show_blockchain(&self) {
        let blockchain = self.blockchain.lock();

        println!("Blockchain Summary:");
        println!("  Length: {} blocks", blockchain.chain.len());
        println!("  Difficulty: {}", blockchain.difficulty);

        if !blockchain.chain.is_empty() {
            let genesis = &blockchain.chain[0];
            println!("  Genesis hash: {}...", &genesis.hash[..16]);

            if blockchain.chain.len() > 1 {
                let latest = blockchain.chain.last().unwrap();
                println!("  Latest hash: {}...", &latest.hash[..16]);
                println!("  Latest timestamp: {}", latest.timestamp);
            }
        }

        println!("  Valid: {}", blockchain.is_valid());
    }

    fn show_stats(&self) {
        let uptime = self.start_time.elapsed().as_secs();
        let peers_count = self.peers.lock().len();
        let blocks_count = self.blockchain.lock().chain.len();

        println!("Bootstrap Node Statistics:");
        println!("  Uptime: {}s ({} min)", uptime, uptime / 60);
        println!("  Connected peers: {}", peers_count);
        println!("  Blockchain length: {} blocks", blocks_count);
        println!("  Status: Running");
    }

    fn show_mempool(&self) {
        let mempool = self.mempool.lock();
        println!("\n=== Transaction Mempool ===");
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

    fn clear_mempool(&self) {
        let mut mempool = self.mempool.lock();
        let size = mempool.size();
        mempool.clear();
        println!("Cleared {} transactions from mempool", size);
    }

    ///block template for mining
    pub fn create_block_template(&self) -> BlockTemplate {
        let blockchain = self.blockchain.lock();
        let mempool = self.mempool.lock();

        let transactions = mempool.get_transactions(10);

        let tx_strings: Vec<String> = transactions
            .iter()
            .map(|tx| serde_json::to_string(tx).unwrap())
            .collect();

        BlockTemplate::new(
            blockchain.last_block().hash.clone(),
            tx_strings,
            blockchain.difficulty,
            blockchain.chain.len(),
        )
    }

    fn initiate_mining_round(&self) {
        if self.mining_active.load(Ordering::SeqCst) {
            println!("Mining already in progress");
            return;
        }

        let peers_count = self.peers.lock().len();
        if peers_count == 0 {
            println!("No peers connected. Cannot start mining.");
            return;
        }

        println!("\nInitiating mining...");

        let template = self.create_block_template();

        println!("Block template created:");
        println!("  Block number: {}", template.block_number);
        println!("  Difficulty: {} leading zeros", template.difficulty);
        println!("  Transactions: {}", template.transactions.len());

        self.mining_active.store(true, Ordering::SeqCst);

        let round_num = self.mining_round.fetch_add(1, Ordering::SeqCst) + 1;
        *self.mining_round_start.lock() = Some(Instant::now());
        self.logger.log(NetworkEvent::MiningRoundStarted {
            round_number: round_num,
            timestamp: chrono::Utc::now().timestamp() as u64,
        });

        let msg = P2PMessage::MiningStart {
            template: template.clone(),
        };

        if let Err(e) = self.network.broadcast(&msg) {
            eprintln!("Failed to broadcast mining start: {}", e);
            self.mining_active.store(false, Ordering::SeqCst);
            return;
        }

        println!("Mining start broadcast to {} peers", peers_count);
    }

    fn stop_mining(&self) {
        if !self.mining_active.load(Ordering::SeqCst) {
            println!("Mining is not active");
            return;
        }

        self.mining_active.store(false, Ordering::SeqCst);
        self.network.broadcast(&P2PMessage::MiningStop).ok();
        println!("Mining stopped");
    }

    fn request_blockchain_sync(&self) {
        println!("Requesting blockchain from all peers...");
        let msg = P2PMessage::RequestBlockchain {
            requester_id: "bootstrap".to_string(),
        };
        if let Err(e) = self.network.broadcast(&msg) {
            eprintln!("Failed to broadcast sync request: {}", e);
        } else {
            println!("Sync request sent to all peers");
        }
    }

    fn show_network_stats(&self) {
        let events = self.logger.get_events();
        let stats = NetworkStatistics::from_events(&events);
        stats.display();
    }

    fn export_logs(&self) {
        println!("\nExport logs:");
        println!("  1. JSON format");
        println!("  2. CSV format");
        print!("Select format: ");
        io::stdout().flush().ok();

        let stdin = io::stdin();
        let mut input = String::new();
        if stdin.read_line(&mut input).is_err() {
            eprintln!("Failed to read input");
            return;
        }

        match input.trim() {
            "1" => {
                let filename = format!("bootstrap_logs_{}.json", chrono::Utc::now().timestamp());
                if let Err(e) = self.logger.export_json(&filename) {
                    eprintln!("Failed to export JSON: {}", e);
                }
            }
            "2" => {
                let filename = format!("bootstrap_logs_{}.csv", chrono::Utc::now().timestamp());
                if let Err(e) = self.logger.export_csv(&filename) {
                    eprintln!("Failed to export CSV: {}", e);
                }
            }
            _ => println!("Invalid selection"),
        }
    }

    fn clear_logs(&self) {
        let count = self.logger.count();
        self.logger.clear();
        println!("Cleared {} logged events", count);
    }
}

/// run bootstrap node (called from main)
pub fn run_bootstrap_node(port: u16) -> Result<(), Box<dyn std::error::Error>> {
    println!("===================================================");
    println!("Bootstrap Node Starting");
    println!("===================================================");
    println!("Topology: Star");
    println!("Port: {}", port);
    println!();

    let mut bootstrap = BootstrapNode::new(port)?;

    println!("Network layer initialized");
    println!("Blockchain initialized (Genesis block)");
    println!("Listening on 0.0.0.0:{}...", port);
    println!("Waiting for nodes to connect...");
    println!();

    bootstrap.start()?;

    Ok(())
}
