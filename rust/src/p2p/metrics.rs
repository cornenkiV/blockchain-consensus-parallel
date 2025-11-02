use crate::p2p::logger::NetworkEvent;
use std::collections::HashMap;

///aggregated network statistics
#[derive(Debug, Clone)]
pub struct NetworkStatistics {
    //mining stats
    pub total_mining_rounds: u64,
    pub total_mining_time_ms: u64,
    pub average_mining_time_ms: f64,
    pub blocks_mined_by_node: HashMap<String, u64>,

    //network stats
    pub total_peers: usize,
    pub current_peers: usize,
    pub total_blocks_received: u64,
    pub total_blocks_added: u64,
    pub total_blocks_rejected: u64,
    pub total_blocks_broadcast: u64,

    //consensus stats
    pub total_reorganizations: u64,
    pub total_chain_syncs: u64,
    pub total_forks_detected: u64,

    //transaction stats
    pub total_transactions_received: u64,
    pub total_transactions_added: u64,
    pub total_transactions_broadcast: u64,
}

impl NetworkStatistics {
    pub fn from_events(events: &[NetworkEvent]) -> Self {
        let mut stats = NetworkStatistics {
            total_mining_rounds: 0,
            total_mining_time_ms: 0,
            average_mining_time_ms: 0.0,
            blocks_mined_by_node: HashMap::new(),
            total_peers: 0,
            current_peers: 0,
            total_blocks_received: 0,
            total_blocks_added: 0,
            total_blocks_rejected: 0,
            total_blocks_broadcast: 0,
            total_reorganizations: 0,
            total_chain_syncs: 0,
            total_forks_detected: 0,
            total_transactions_received: 0,
            total_transactions_added: 0,
            total_transactions_broadcast: 0,
        };

        let mut peer_count = 0i32;

        for event in events {
            match event {
                NetworkEvent::PeerConnected { .. } => {
                    stats.total_peers += 1;
                    peer_count += 1;
                }
                NetworkEvent::PeerDisconnected { .. } => {
                    peer_count -= 1;
                }

                NetworkEvent::MiningRoundStarted { .. } => {
                    stats.total_mining_rounds += 1;
                }
                NetworkEvent::MiningRoundCompleted {
                    winner_id,
                    duration_ms,
                    ..
                } => {
                    stats.total_mining_time_ms += duration_ms;
                    *stats
                        .blocks_mined_by_node
                        .entry(winner_id.clone())
                        .or_insert(0) += 1;
                }

                NetworkEvent::BlockReceived { .. } => {
                    stats.total_blocks_received += 1;
                }
                NetworkEvent::BlockAdded { .. } => {
                    stats.total_blocks_added += 1;
                }
                NetworkEvent::BlockRejected { .. } => {
                    stats.total_blocks_rejected += 1;
                }
                NetworkEvent::BlockBroadcast { .. } => {
                    stats.total_blocks_broadcast += 1;
                }

                NetworkEvent::ChainReorganization { .. } => {
                    stats.total_reorganizations += 1;
                }
                NetworkEvent::ChainSyncRequested { .. }
                | NetworkEvent::ChainSyncReceived { .. } => {
                    stats.total_chain_syncs += 1;
                }
                NetworkEvent::ForkDetected { .. } => {
                    stats.total_forks_detected += 1;
                }

                NetworkEvent::TransactionReceived { .. } => {
                    stats.total_transactions_received += 1;
                }
                NetworkEvent::TransactionAdded { .. } => {
                    stats.total_transactions_added += 1;
                }
                NetworkEvent::TransactionBroadcast { .. } => {
                    stats.total_transactions_broadcast += 1;
                }

                _ => {}
            }
        }

        if stats.total_mining_rounds > 0 {
            stats.average_mining_time_ms =
                stats.total_mining_time_ms as f64 / stats.total_mining_rounds as f64;
        }

        stats.current_peers = peer_count.max(0) as usize;

        stats
    }

    pub fn display(&self) {
        println!("\n===============================================");
        println!("NETWORK STATISTICS");
        println!("===============================================");
        println!("\nMINING STATISTICS:");
        println!(
            "  - Total mining rounds:        {}",
            self.total_mining_rounds
        );
        println!(
            "  -Total mining time:          {} ms",
            self.total_mining_time_ms
        );
        println!(
            "  - Average mining time:        {:.2} ms",
            self.average_mining_time_ms
        );
        println!("  - Blocks mined by node:");
        let mut mined_vec: Vec<_> = self.blocks_mined_by_node.iter().collect();
        mined_vec.sort_by_key(|(_, count)| std::cmp::Reverse(**count));
        for (node_id, count) in mined_vec {
            println!("      - {}: {} blocks", node_id, count);
        }

        println!("\nNETWORK STATISTICS:");
        println!("  - Total peers connected:      {}", self.total_peers);
        println!("  - Current active peers:       {}", self.current_peers);
        println!(
            "  - Blocks received:            {}",
            self.total_blocks_received
        );
        println!(
            "  - Blocks added to chain:      {}",
            self.total_blocks_added
        );
        println!(
            "  - Blocks rejected:            {}",
            self.total_blocks_rejected
        );
        println!(
            "  - Blocks broadcast:           {}",
            self.total_blocks_broadcast
        );

        println!("\n⚖️  CONSENSUS STATISTICS:");
        println!(
            "  - Chain reorganizations:      {}",
            self.total_reorganizations
        );
        println!("  - Chain synchronizations:     {}", self.total_chain_syncs);
        println!(
            "  - Forks detected:             {}",
            self.total_forks_detected
        );

        println!("\nTRANSACTION STATISTICS:");
        println!(
            "  - Transactions received:      {}",
            self.total_transactions_received
        );
        println!(
            "  - Transactions added:         {}",
            self.total_transactions_added
        );
        println!(
            "  - Transactions broadcast:     {}",
            self.total_transactions_broadcast
        );

        println!();
    }
}
