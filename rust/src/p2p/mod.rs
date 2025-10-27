/// P2P module
pub mod error;
pub mod network;
pub mod protocol;

pub use error::NetworkError;
pub use network::{NetworkLayer, StarNetworkClient, StarNetworkServer};
pub use protocol::{BlockTemplate, P2PMessage, PeerInfo};
