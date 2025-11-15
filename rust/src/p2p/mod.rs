/// P2P module
pub mod bootstrap;
pub mod error;
pub mod gossip;
pub mod logger;
pub mod mempool;
pub mod mesh_node;
pub mod metrics;
pub mod mining;
pub mod network;
pub mod node;
pub mod protocol;

pub use bootstrap::run_bootstrap_node;
pub use error::NetworkError;
pub use gossip::{MessageTracker, compute_message_id};
pub use logger::{NetworkEvent, NetworkLogger};
pub use mempool::TransactionPool;
pub use mesh_node::run_mesh_node;
pub use metrics::NetworkStatistics;
pub use network::{MeshNetwork, NetworkLayer, StarNetworkClient, StarNetworkServer};
pub use node::run_regular_node;
pub use protocol::{BlockTemplate, P2PMessage, PeerInfo};
