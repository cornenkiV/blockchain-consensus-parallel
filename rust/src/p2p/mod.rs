/// P2P module
pub mod bootstrap;
pub mod error;
pub mod mempool;
pub mod network;
pub mod node;
pub mod protocol;

pub use bootstrap::run_bootstrap_node;
pub use error::NetworkError;
pub use mempool::TransactionPool;
pub use network::{NetworkLayer, StarNetworkClient, StarNetworkServer};
pub use node::run_regular_node;
pub use protocol::{BlockTemplate, P2PMessage, PeerInfo};
