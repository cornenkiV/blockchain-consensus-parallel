use crate::blockchain::Blockchain;
use crate::p2p::error::NetworkError;
use crate::p2p::logger::{NetworkEvent, NetworkLogger};
use crate::p2p::mempool::TransactionPool;
use crate::p2p::metrics::NetworkStatistics;
use crate::p2p::network::{MeshNetwork, NetworkLayer};
use crate::p2p::protocol::{BlockTemplate, P2PMessage, PeerInfo};
use crate::pos::Transaction;
use parking_lot::Mutex;
use rand::seq::SliceRandom;
use std::io::{self, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

pub struct MeshNode {
    node_id: String,
    network: Arc<MeshNetwork>,
    bootstrap_address: String,
    blockchain: Arc<Mutex<Blockchain>>,
    mempool: Arc<Mutex<TransactionPool>>,
    peers: Arc<Mutex<Vec<PeerInfo>>>,
    running: Arc<AtomicBool>,
    logger: NetworkLogger,
    mining_active: Arc<AtomicBool>,
    mining_stop_flag: Arc<AtomicBool>,
    mining_thread: Arc<Mutex<Option<JoinHandle<()>>>>,
    mining_start_time: Arc<Mutex<Option<Instant>>>,
    listener: Option<TcpListener>,
}

impl MeshNode {
    pub fn new(
        bootstrap_address: &str,
        max_peers: usize,
        listen_port: u16,
    ) -> Result<Self, NetworkError> {
        let node_id = format!("mesh_node_{}", rand::random::<u16>());
        let network = Arc::new(MeshNetwork::new(node_id.clone(), max_peers));

        let blockchain = Blockchain {
            chain: Vec::new(),
            difficulty: 5,
        };
        let mempool = TransactionPool::new(1000);

        let listener_addr = format!("0.0.0.0:{}", listen_port);
        let listener = TcpListener::bind(&listener_addr).map_err(|e| {
            NetworkError::ConnectionFailed(format!("Failed to bind to {}: {}", listener_addr, e))
        })?;

        listener.set_nonblocking(true).map_err(|e| {
            NetworkError::ConnectionFailed(format!("Failed to set non-blocking: {}", e))
        })?;

        println!("Mesh node listening on {}", listener_addr);

        Ok(MeshNode {
            node_id,
            network,
            bootstrap_address: bootstrap_address.to_string(),
            blockchain: Arc::new(Mutex::new(blockchain)),
            mempool: Arc::new(Mutex::new(mempool)),
            peers: Arc::new(Mutex::new(Vec::new())),
            running: Arc::new(AtomicBool::new(true)),
            logger: NetworkLogger::new(),
            mining_active: Arc::new(AtomicBool::new(false)),
            mining_stop_flag: Arc::new(AtomicBool::new(false)),
            mining_thread: Arc::new(Mutex::new(None)),
            mining_start_time: Arc::new(Mutex::new(None)),
            listener: Some(listener),
        })
    }

    pub fn connect(&mut self) -> Result<(), NetworkError> {
        println!(
            "Connecting to bootstrap for peer discovery: {}",
            self.bootstrap_address
        );

        let mut bootstrap_stream = TcpStream::connect(&self.bootstrap_address).map_err(|e| {
            NetworkError::ConnectionFailed(format!("Failed to connect to bootstrap: {}", e))
        })?;

        let join = P2PMessage::Join {
            node_id: self.node_id.clone(),
            address: format!(
                "127.0.0.1:{}",
                self.listener.as_ref().unwrap().local_addr().unwrap().port()
            ),
            timestamp: chrono::Utc::now().timestamp() as u64,
        };
        join.send(&mut bootstrap_stream)
            .map_err(|e| NetworkError::SendFailed(format!("Failed to send Join: {}", e)))?;

        let request = P2PMessage::RequestBlockchain {
            requester_id: self.node_id.clone(),
        };
        request.send(&mut bootstrap_stream).map_err(|e| {
            NetworkError::SendFailed(format!("Failed to request blockchain: {}", e))
        })?;

        bootstrap_stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .ok();

        let mut received_blockchain = false;
        let mut received_peer_list = false;

        for _ in 0..5 {
            match P2PMessage::receive(&mut bootstrap_stream) {
                Ok(P2PMessage::BlockchainSync { chain }) => {
                    println!("Received blockchain ({} blocks)", chain.len());
                    let mut blockchain_lock = self.blockchain.lock();
                    blockchain_lock.chain = chain;
                    received_blockchain = true;

                    if received_peer_list {
                        break;
                    }
                }
                Ok(P2PMessage::PeerList { peers }) => {
                    println!("Received peer list ({} peers)", peers.len());
                    self.handle_peer_list(peers);
                    received_peer_list = true;

                    if received_blockchain {
                        break;
                    }
                }
                Ok(msg) => {
                    println!("Unexpected message from bootstrap: {:?}", msg);
                }
                Err(e) => {
                    let err_str = e.to_string();
                    if err_str.contains("timed out")
                        || err_str.contains("WouldBlock")
                        || err_str.contains("10060")
                    {
                        break;
                    }
                    println!("Error receiving from bootstrap: {}", e);
                    break;
                }
            }
        }

        drop(bootstrap_stream);

        println!(
            "Peer discovery complete, connected to {} peers",
            self.network.peer_count()
        );

        Ok(())
    }

    fn connect_to_peer_with_loop(
        &self,
        peer_address: &str,
        peer_id: &str,
    ) -> Result<(), NetworkError> {
        if peer_id == self.node_id {
            return Ok(());
        }

        if self
            .network
            .get_connected_peers()
            .contains(&peer_id.to_string())
        {
            return Ok(());
        }

        let mut stream = TcpStream::connect(peer_address).map_err(|e| {
            NetworkError::ConnectionFailed(format!("Failed to connect to {}: {}", peer_address, e))
        })?;

        let join = P2PMessage::Join {
            node_id: self.node_id.clone(),
            address: format!(
                "127.0.0.1:{}",
                self.listener.as_ref().unwrap().local_addr().unwrap().port()
            ),
            timestamp: chrono::Utc::now().timestamp() as u64,
        };
        join.send(&mut stream)
            .map_err(|e| NetworkError::SendFailed(format!("Failed to send Join: {}", e)))?;

        let mut stream_for_network = stream.try_clone().map_err(|e| {
            NetworkError::ConnectionFailed(format!("Failed to clone stream: {}", e))
        })?;

        stream
            .set_read_timeout(Some(Duration::from_secs(1)))
            .map_err(|e| NetworkError::ConnectionFailed(format!("Failed to set timeout: {}", e)))?;

        stream_for_network
            .set_read_timeout(Some(Duration::from_secs(1)))
            .map_err(|e| {
                NetworkError::ConnectionFailed(format!("Failed to set timeout on clone: {}", e))
            })?;

        self.network
            .accept_peer(peer_id.to_string(), stream_for_network);

        Self::start_peer_message_loop_static(
            peer_id.to_string(),
            stream,
            self.network.clone(),
            self.blockchain.clone(),
            self.peers.clone(),
            self.mempool.clone(),
            self.mining_active.clone(),
            self.mining_stop_flag.clone(),
            self.mining_thread.clone(),
            self.running.clone(),
            self.logger.clone(),
            self.mining_start_time.clone(),
            self.node_id.clone(),
        );

        Ok(())
    }

    fn handle_peer_list(&mut self, peers: Vec<PeerInfo>) {
        println!("Received {} peers from bootstrap", peers.len());

        *self.peers.lock() = peers.clone();

        let mut rng = rand::thread_rng();
        let num_to_connect = std::cmp::min(3, peers.len());
        let peers_to_connect: Vec<_> = peers.choose_multiple(&mut rng, num_to_connect).collect();

        let mut successfully_connected = Vec::new();

        for peer in peers_to_connect {
            if peer.node_id == self.node_id {
                continue;
            }

            println!("Attempting to connect to peer: {}", peer.node_id);
            match self.connect_to_peer_with_loop(&peer.address, &peer.node_id) {
                Ok(_) => {
                    println!("Connected and started message loop for {}", peer.node_id);
                    successfully_connected.push(peer.node_id.clone());
                }
                Err(e) => {
                    println!(
                        "Failed to connect to {} (node may be offline): {}",
                        peer.node_id, e
                    );
                }
            }
        }

        if let Some(peer_id) = successfully_connected.first() {
            println!("Requesting blockchain from mesh peer: {}", peer_id);
            let request = P2PMessage::RequestBlockchain {
                requester_id: self.node_id.clone(),
            };
            if let Err(e) = self.network.send_to(peer_id, &request) {
                println!("Failed to request blockchain from {}: {}", peer_id, e);
            }
        }
    }

    pub fn start(&mut self) -> Result<(), NetworkError> {
        let running = self.running.clone();
        ctrlc::set_handler(move || {
            println!("\n\nShutting down...");
            println!("(Press Enter to complete shutdown)");
            running.store(false, Ordering::SeqCst);
        })
        .expect("Error");

        self.start_listener_thread();

        self.start_peer_message_threads();

        self.start_heartbeat_thread();

        self.cli_loop();

        Ok(())
    }

    fn start_listener_thread(&self) {
        let listener = self.listener.as_ref().unwrap().try_clone().unwrap();
        let network = self.network.clone();
        let running = self.running.clone();
        let blockchain = self.blockchain.clone();
        let peers = self.peers.clone();
        let mempool = self.mempool.clone();
        let mining_active = self.mining_active.clone();
        let mining_stop_flag = self.mining_stop_flag.clone();
        let mining_thread = self.mining_thread.clone();
        let logger = self.logger.clone();
        let mining_start_time = self.mining_start_time.clone();
        let node_id = self.node_id.clone();

        thread::spawn(move || {
            println!("Listener thread started");

            while running.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, addr)) => {
                        println!("Incoming connection from {}", addr);

                        match P2PMessage::receive(&mut stream) {
                            Ok(P2PMessage::Join {
                                node_id: peer_id,
                                address: _,
                                timestamp: _,
                            }) => {
                                println!("Peer joining: {}", peer_id);

                                let mut stream_clone = stream.try_clone().unwrap();

                                stream.set_read_timeout(Some(Duration::from_secs(1))).ok();
                                stream_clone
                                    .set_read_timeout(Some(Duration::from_secs(1)))
                                    .ok();

                                network.accept_peer(peer_id.clone(), stream_clone);

                                Self::start_peer_message_loop_static(
                                    peer_id.clone(),
                                    stream,
                                    network.clone(),
                                    blockchain.clone(),
                                    peers.clone(),
                                    mempool.clone(),
                                    mining_active.clone(),
                                    mining_stop_flag.clone(),
                                    mining_thread.clone(),
                                    running.clone(),
                                    logger.clone(),
                                    mining_start_time.clone(),
                                    node_id.clone(),
                                );
                            }
                            Ok(_) => {
                                println!("Unexpected first message from {}", addr);
                            }
                            Err(e) => {
                                println!("Failed to receive Join from {}: {}", addr, e);
                            }
                        }
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(100));
                    }
                    Err(e) => {
                        if running.load(Ordering::Relaxed) {
                            eprintln!("Listener error: {}", e);
                        }
                    }
                }
            }

            println!("Listener thread stopped");
        });
    }

    fn start_peer_message_threads(&self) {
        let connected_peers = self.network.get_connected_peers();
        println!("Starting message loops for {} peers", connected_peers.len());
    }

    fn start_heartbeat_thread(&self) {
        let running = self.running.clone();
        let bootstrap_address = self.bootstrap_address.clone();
        let node_id = self.node_id.clone();

        std::thread::spawn(move || {
            println!("Heartbeat thread started (sending every 20s)");

            while running.load(Ordering::Relaxed) {
                for _ in 0..20 {
                    if !running.load(Ordering::Relaxed) {
                        break;
                    }
                    std::thread::sleep(Duration::from_secs(1));
                }

                if !running.load(Ordering::Relaxed) {
                    break;
                }

                let timestamp = chrono::Utc::now().timestamp() as u64;
                let heartbeat_msg = P2PMessage::Heartbeat {
                    node_id: node_id.clone(),
                    timestamp,
                };

                match TcpStream::connect(&bootstrap_address) {
                    Ok(mut stream) => {
                        if let Err(e) = heartbeat_msg.send(&mut stream) {
                            eprintln!("Failed to send heartbeat: {}", e);
                        }
                    }
                    Err(_) => {}
                }
            }

            println!("Heartbeat thread stopped");
        });
    }

    fn start_peer_message_loop_static(
        peer_id: String,
        mut stream: TcpStream,
        network: Arc<MeshNetwork>,
        blockchain: Arc<Mutex<Blockchain>>,
        peers: Arc<Mutex<Vec<PeerInfo>>>,
        mempool: Arc<Mutex<TransactionPool>>,
        mining_active: Arc<AtomicBool>,
        mining_stop_flag: Arc<AtomicBool>,
        mining_thread: Arc<Mutex<Option<JoinHandle<()>>>>,
        running: Arc<AtomicBool>,
        logger: NetworkLogger,
        mining_start_time: Arc<Mutex<Option<Instant>>>,
        node_id: String,
    ) {
        thread::spawn(move || {
            println!("Message loop started for peer: {}", peer_id);

            while running.load(Ordering::Relaxed) {
                match P2PMessage::receive(&mut stream) {
                    Ok(message) => {
                        Self::handle_message_static(
                            &node_id,
                            &peer_id,
                            message,
                            &network,
                            &blockchain,
                            &peers,
                            &mempool,
                            &mining_active,
                            &mining_stop_flag,
                            &mining_thread,
                            &logger,
                            &mining_start_time,
                        );
                    }
                    Err(e) => {
                        let error_str = e.to_string();

                        if error_str.contains("timed out")
                            || error_str.contains("WouldBlock")
                            || error_str.contains("10060")
                        {
                            continue;
                        }

                        println!("Peer {} disconnected: {}", peer_id, e);
                        network.remove_peer(&peer_id);
                        break;
                    }
                }
            }

            println!("Message loop stopped for peer: {}", peer_id);
        });
    }

    fn handle_message_static(
        node_id: &str,
        from_peer: &str,
        message: P2PMessage,
        network: &Arc<MeshNetwork>,
        blockchain: &Arc<Mutex<Blockchain>>,
        _peers: &Arc<Mutex<Vec<PeerInfo>>>,
        mempool: &Arc<Mutex<TransactionPool>>,
        mining_active: &Arc<AtomicBool>,
        mining_stop_flag: &Arc<AtomicBool>,
        mining_thread: &Arc<Mutex<Option<JoinHandle<()>>>>,
        logger: &NetworkLogger,
        mining_start_time: &Arc<Mutex<Option<Instant>>>,
    ) {
        match message.clone() {
            P2PMessage::BlockchainSync { chain } => {
                println!("Received blockchain ({} blocks)", chain.len());

                let timestamp = chrono::Utc::now().timestamp() as u64;
                logger.log(NetworkEvent::ChainSyncReceived {
                    chain_length: chain.len(),
                    timestamp,
                });

                let mut blockchain_lock = blockchain.lock();

                if blockchain_lock.chain.is_empty() {
                    if !Blockchain::validate_chain(&chain, blockchain_lock.difficulty) {
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
                    return;
                }

                if !Blockchain::validate_chain(&chain, blockchain_lock.difficulty) {
                    eprintln!("   Chain validation failed");
                    return;
                }

                let old_height = blockchain_lock.chain.len();
                logger.log(NetworkEvent::ChainReorganization {
                    old_height,
                    new_height: chain.len(),
                    timestamp,
                });

                blockchain_lock.reorganize(chain);
            }

            P2PMessage::NewBlock { block, miner_id } => {
                let timestamp = chrono::Utc::now().timestamp() as u64;
                {
                    let blockchain_lock = blockchain.lock();
                    if blockchain_lock.chain.iter().any(|b| b.hash == block.hash) {
                        return;
                    }
                }

                let block_height = blockchain.lock().chain.len();

                logger.log(NetworkEvent::BlockReceived {
                    block_height,
                    block_hash: block.hash.clone(),
                    miner_id: miner_id.clone(),
                    timestamp,
                });

                let is_valid = blockchain.lock().validate_block(&block);

                if is_valid {
                    {
                        let mut blockchain_lock = blockchain.lock();
                        blockchain_lock.add_block(block.clone());
                        let new_height = blockchain_lock.chain.len() - 1;
                        println!("\nNew block added to chain (mined by {})", miner_id);
                        println!("  Block #{}: {}...", new_height, &block.hash[..16]);

                        logger.log(NetworkEvent::BlockAdded {
                            block_height: new_height,
                            block_hash: block.hash.clone(),
                            timestamp,
                        });
                    }

                    mining_stop_flag.store(true, Ordering::SeqCst);
                    mining_active.store(false, Ordering::SeqCst);

                    logger.log(NetworkEvent::MiningStopped {
                        node_id: node_id.to_string(),
                        reason: "block_found_by_peer".to_string(),
                        timestamp,
                    });

                    mempool.lock().clear();
                } else {
                    println!("Received block doesn't fit current chain");

                    logger.log(NetworkEvent::ForkDetected {
                        block_height,
                        timestamp,
                    });
                }
            }

            P2PMessage::MiningStart { template } => {
                if !mining_active.load(Ordering::Relaxed) {
                    Self::handle_mining_start(
                        node_id,
                        template,
                        network,
                        blockchain,
                        mempool,
                        mining_active,
                        mining_stop_flag,
                        mining_thread,
                        logger,
                        mining_start_time,
                    );
                }
            }

            P2PMessage::MiningStop => {
                println!("Mining stop signal received");
                mining_stop_flag.store(true, Ordering::SeqCst);
                mining_active.store(false, Ordering::SeqCst);

                logger.log(NetworkEvent::MiningStopped {
                    node_id: node_id.to_string(),
                    reason: "stop_signal_received".to_string(),
                    timestamp: chrono::Utc::now().timestamp() as u64,
                });
            }

            P2PMessage::NewTransaction {
                transaction,
                from_node,
            } => {
                let tx: Transaction = match serde_json::from_str(&transaction) {
                    Ok(tx) => tx,
                    Err(_) => return,
                };

                let tx_hash = format!("{:x}", md5::compute(transaction.as_bytes()));
                let timestamp = chrono::Utc::now().timestamp() as u64;

                logger.log(NetworkEvent::TransactionReceived {
                    tx_hash: tx_hash.clone(),
                    timestamp,
                });

                let mut mempool_lock = mempool.lock();
                if mempool_lock.add_transaction(tx.clone()).is_ok() {
                    logger.log(NetworkEvent::TransactionAdded { tx_hash, timestamp });

                    println!("New transaction received (from {})", from_node);
                    println!("   {} -> {}: {} coins", tx.from, tx.to, tx.amount);

                    let mempool_size = mempool_lock.size();
                    println!("   Mempool size: {}/10", mempool_size);

                    // mining start when mempool has 10 transactions
                    if mempool_size >= 10 && !mining_active.load(Ordering::Relaxed) {
                        drop(mempool_lock);

                        println!("Mining started");

                        let template = {
                            let blockchain_lock = blockchain.lock();
                            let mempool_lock = mempool.lock();
                            let transactions = mempool_lock.get_transactions(10);
                            let tx_strings: Vec<String> = transactions
                                .iter()
                                .map(|tx| serde_json::to_string(tx).unwrap())
                                .collect();

                            BlockTemplate::new(
                                blockchain_lock.last_block().hash.clone(),
                                tx_strings,
                                blockchain_lock.difficulty,
                                blockchain_lock.chain.len(),
                            )
                        };

                        let mining_msg = P2PMessage::MiningStart {
                            template: template.clone(),
                        };
                        network.gossip_broadcast(&mining_msg, Some(node_id)).ok();

                        Self::handle_mining_start(
                            node_id,
                            template,
                            network,
                            blockchain,
                            mempool,
                            mining_active,
                            mining_stop_flag,
                            mining_thread,
                            logger,
                            mining_start_time,
                        );
                    }
                }
            }

            P2PMessage::RequestBlockchain { requester_id } => {
                println!("Peer {} requesting blockchain", requester_id);

                let chain = blockchain.lock().chain.clone();
                let sync_msg = P2PMessage::BlockchainSync {
                    chain: chain.clone(),
                };

                if let Err(e) = network.send_to(&requester_id, &sync_msg) {
                    eprintln!("Failed to send blockchain to {}: {}", requester_id, e);
                } else {
                    println!(
                        "Sent blockchain to {} ({} blocks)",
                        requester_id,
                        chain.len()
                    );
                }
            }

            _ => {}
        }

        network.gossip_broadcast(&message, Some(from_peer)).ok();
    }

    fn handle_mining_start(
        node_id: &str,
        template: BlockTemplate,
        network: &Arc<MeshNetwork>,
        blockchain: &Arc<Mutex<Blockchain>>,
        mempool: &Arc<Mutex<TransactionPool>>,
        mining_active: &Arc<AtomicBool>,
        mining_stop_flag: &Arc<AtomicBool>,
        mining_thread: &Arc<Mutex<Option<JoinHandle<()>>>>,
        logger: &NetworkLogger,
        mining_start_time: &Arc<Mutex<Option<Instant>>>,
    ) {
        println!("\nMining start signal received");
        println!(
            "  Block #{}, Difficulty: {}",
            template.block_number, template.difficulty
        );

        // prevent multiple threads from starting mining simultaneously
        if mining_active
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::Relaxed)
            .is_err()
        {
            return;
        }

        mining_stop_flag.store(true, Ordering::SeqCst);
        if let Some(handle) = mining_thread.lock().take() {
            handle.join().ok();
        }

        mining_stop_flag.store(false, Ordering::SeqCst);

        let timestamp = chrono::Utc::now().timestamp() as u64;
        logger.log(NetworkEvent::MiningStarted {
            node_id: node_id.to_string(),
            timestamp,
        });

        *mining_start_time.lock() = Some(Instant::now());

        let node_id = node_id.to_string();
        let network = network.clone();
        let stop_flag = Arc::clone(mining_stop_flag);
        let mining_active_clone = Arc::clone(mining_active);
        let blockchain = blockchain.clone();
        let mempool = mempool.clone();
        let logger = logger.clone();

        let handle = thread::spawn(move || {
            println!("Mining started...");
            let start_time = Instant::now();

            let result = crate::p2p::mining::mine_block_parallel(template, 4, stop_flag.clone());

            if let Some(block) = result {
                let elapsed = start_time.elapsed().as_secs_f64();
                println!("\nBLOCK FOUND!");
                println!("  Nonce: {}", block.nonce);
                println!("  Hash: {}...", &block.hash[..16]);
                println!("  Time: {:.2}s", elapsed);

                let is_valid = blockchain.lock().validate_block(&block);

                if is_valid {
                    {
                        let mut blockchain_lock = blockchain.lock();
                        blockchain_lock.add_block(block.clone());
                        let new_height = blockchain_lock.chain.len() - 1;
                        println!("  Block #{} added to local chain", new_height);

                        let timestamp = chrono::Utc::now().timestamp() as u64;
                        logger.log(NetworkEvent::BlockAdded {
                            block_height: new_height,
                            block_hash: block.hash.clone(),
                            timestamp,
                        });
                    }

                    mempool.lock().clear();
                    println!("  Mempool cleared");

                    let msg = P2PMessage::NewBlock {
                        block,
                        miner_id: node_id.clone(),
                    };

                    if let Err(e) = network.broadcast(&msg) {
                        eprintln!("Failed to broadcast block: {}", e);
                    } else {
                        println!("Block broadcast to mesh network");
                    }
                } else {
                    println!("Block is no longer valid (chain changed during mining), discarding");
                }
            } else {
                println!("Mining stopped (block found by another node)");
            }

            mining_active_clone.store(false, Ordering::SeqCst);
        });

        *mining_thread.lock() = Some(handle);
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
                "add-tx" => self.add_transaction_interactive(),
                "mempool" => self.show_mempool(),
                "mining-status" => self.show_mining_status(),
                "network-stats" => self.show_network_stats(),
                "export-logs" => self.export_logs(),
                "clear-logs" => self.clear_logs(),
                "help" => self.show_help(),
                "exit" | "quit" => {
                    println!("Shutting down node...");
                    println!("Closing connections...");
                    break;
                }
                "" => {}
                _ => println!("Unknown command"),
            }
        }

        self.running.store(false, Ordering::SeqCst);
        println!("Node stopped");
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
        let connected = self.network.get_connected_peers();
        println!("\n=== Connected Mesh Peers ===");
        println!("Connected to {} peers:", connected.len());

        for peer_id in connected {
            println!("  - {}", peer_id);
        }
        println!();
    }

    fn show_status(&self) {
        let blockchain = self.blockchain.lock();
        let connected_peers = self.network.get_connected_peers();

        println!("\n=== Mesh Node Status ===");
        println!("Node ID: {}", self.node_id);
        println!("Topology: MESH");
        println!("Blockchain: {} blocks", blockchain.chain.len());
        println!("Peers: {}", connected_peers.len());
        println!("Status: Running");
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

        let mempool_size = {
            let mut mempool = self.mempool.lock();
            if let Err(e) = mempool.add_transaction(tx.clone()) {
                println!("Failed: {}", e);
                return;
            }
            mempool.size()
        };

        println!("   Mempool size: {}/10", mempool_size);

        let tx_json = serde_json::to_string(&tx).unwrap();
        let tx_hash = format!("{:x}", md5::compute(tx_json.as_bytes()));

        let msg = P2PMessage::NewTransaction {
            transaction: tx_json,
            from_node: self.node_id.clone(),
        };

        if let Err(e) = self.network.broadcast(&msg) {
            println!("Failed to broadcast: {}", e);
        } else {
            self.logger.log(NetworkEvent::TransactionBroadcast {
                tx_hash,
                timestamp: chrono::Utc::now().timestamp() as u64,
            });
            println!("Transaction added and broadcast to mesh network");
        }

        if mempool_size >= 10 && !self.mining_active.load(Ordering::Relaxed) {
            println!("\nStarting mining");
            self.trigger_mining();
        }
    }

    fn trigger_mining(&self) {
        let template = {
            let blockchain_lock = self.blockchain.lock();
            let mempool_lock = self.mempool.lock();
            let transactions = mempool_lock.get_transactions(10);
            let tx_strings: Vec<String> = transactions
                .iter()
                .map(|tx| serde_json::to_string(tx).unwrap())
                .collect();

            BlockTemplate::new(
                blockchain_lock.last_block().hash.clone(),
                tx_strings,
                blockchain_lock.difficulty,
                blockchain_lock.chain.len(),
            )
        };

        println!(
            "  Block #{}, Difficulty: {}, Transactions: {}",
            template.block_number,
            template.difficulty,
            template.transactions.len()
        );

        let mining_msg = P2PMessage::MiningStart {
            template: template.clone(),
        };

        if let Err(e) = self.network.broadcast(&mining_msg) {
            eprintln!("Failed to broadcast mining start: {}", e);
        }

        Self::handle_mining_start(
            &self.node_id,
            template,
            &self.network,
            &self.blockchain,
            &self.mempool,
            &self.mining_active,
            &self.mining_stop_flag,
            &self.mining_thread,
            &self.logger,
            &self.mining_start_time,
        );
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

    fn show_network_stats(&self) {
        let events = self.logger.get_events();
        let stats = NetworkStatistics::from_events(&events);
        stats.display();
    }

    fn export_logs(&self) {
        println!("\nExport logs:");
        println!("  1. JSON format");
        println!("  2. CSV format");
        print!("Select format (1/2): ");
        io::stdout().flush().ok();

        let stdin = io::stdin();
        let mut input = String::new();
        if stdin.read_line(&mut input).is_err() {
            eprintln!("Failed to read input");
            return;
        }

        match input.trim() {
            "1" => {
                let filename = format!(
                    "{}_logs_{}.json",
                    self.node_id,
                    chrono::Utc::now().timestamp()
                );
                if let Err(e) = self.logger.export_json(&filename) {
                    eprintln!("Failed to export JSON: {}", e);
                }
            }
            "2" => {
                let filename = format!(
                    "{}_logs_{}.csv",
                    self.node_id,
                    chrono::Utc::now().timestamp()
                );
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

    fn show_help(&self) {
        println!("\n=== Available Commands ===");
        println!("  blockchain     - Show blockchain");
        println!("  peers          - Show connected mesh peers");
        println!("  status         - Show node status");
        println!("  add-tx         - Add new transaction");
        println!("  mempool        - Show pending transactions");
        println!("  mining-status  - Show mining status");
        println!("  network-stats  - Show network statistics from events");
        println!("  export-logs    - Export logs to JSON/CSV");
        println!("  clear-logs     - Clear all logged events");
        println!("  help           - Show all commands");
        println!("  exit           - Shutdown node");
        println!();
    }
}

pub fn run_mesh_node(
    bootstrap_address: &str,
    max_peers: usize,
    listen_port: u16,
) -> Result<(), NetworkError> {
    println!("================================================");
    println!("Mesh Node Starting");
    println!("================================================");
    println!("Topology: MESH");
    println!("Max Peers: {}", max_peers);
    println!("Listen Port: {}", listen_port);
    println!();

    let mut node = MeshNode::new(bootstrap_address, max_peers, listen_port)?;

    node.connect()?;

    println!("\nNode ready!");

    node.start()?;

    Ok(())
}
