use crate::blockchain::Blockchain;
use crate::p2p::error::NetworkError;
use crate::p2p::mempool::TransactionPool;
use crate::p2p::network::{NetworkLayer, StarNetworkServer};
use crate::p2p::protocol::{BlockTemplate, P2PMessage, PeerInfo};
use crate::pos::Transaction;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::io::{self, Write};
use std::net::TcpStream;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

/// coordinator for star topology P2P network
pub struct BootstrapNode {
    network: Arc<StarNetworkServer>,
    blockchain: Arc<Mutex<Blockchain>>,
    peers: Arc<Mutex<HashMap<String, PeerInfo>>>,
    mempool: Arc<Mutex<TransactionPool>>,
    running: Arc<AtomicBool>,
    start_time: Instant,
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
            running: Arc::new(AtomicBool::new(true)),
            start_time: Instant::now(),
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
        println!("  stats        - Show node statistics");
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
                "stats" => self.show_stats(),
                "quit" => {
                    println!("Shutting down bootstrap node...");
                    break;
                }
                "" => continue,
                cmd => println!(
                    "Unknown command: {}. Type 'peers', 'blockchain', 'stats', 'mempool', 'clear-mempool', or 'exit'",
                    cmd
                ),
            }
        }

        Ok(())
    }

    fn start_connection_handler(&self) {
        let network = self.network.clone();
        let blockchain = self.blockchain.clone();
        let peers = self.peers.clone();
        let mempool = self.mempool.clone();
        let running = self.running.clone();

        thread::spawn(move || {
            while running.load(Ordering::Relaxed) {
                match network.accept_connection() {
                    Ok((node_id, stream)) => {
                        network.register_peer(node_id.clone(), stream.try_clone().unwrap());
                        {
                            let mut peers_lock = peers.lock();
                            peers_lock.insert(
                                node_id.clone(),
                                PeerInfo {
                                    node_id: node_id.clone(),
                                    address: "test".to_string(),
                                    last_seen: chrono::Utc::now().timestamp() as u64,
                                    blocks_mined: 0,
                                },
                            );
                        }

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
                            running.clone(),
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
        running: Arc<AtomicBool>,
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
                        );
                    }
                    Err(e) => {
                        eprintln!("Error receiving from {}: {}", node_id, e);
                        // peer disconnected
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
    ) {
        match message {
            P2PMessage::RequestBlockchain { requester_id } => {
                println!("{} requested blockchain", requester_id);
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

                //save it to mempool
                let mut mempool_lock = mempool.lock();
                if let Err(e) = mempool_lock.add_transaction(tx.clone()) {
                    eprintln!("Failed to add transaction from {}: {}", from_node, e);
                    return;
                }

                println!("New transaction added to mempool (from {})", from_node);
                println!("   {} -> {}: {} coins", tx.from, tx.to, tx.amount);
                println!("   Mempool size: {}", mempool_lock.size());
                drop(mempool_lock);

                let relay_msg = P2PMessage::NewTransaction {
                    transaction,
                    from_node,
                };
                network.broadcast(&relay_msg).ok();
            }

            P2PMessage::Pong { node_id } => {
                println!("Pong from {}", node_id);
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
