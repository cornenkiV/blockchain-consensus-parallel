use crate::p2p::error::NetworkError;
use crate::p2p::protocol::P2PMessage;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;

pub trait NetworkLayer: Send + Sync {
    /// send message to all connected peers
    fn broadcast(&self, message: &P2PMessage) -> Result<(), NetworkError>;

    /// send message to specific peer by id
    fn send_to(&self, node_id: &str, message: &P2PMessage) -> Result<(), NetworkError>;

    /// get all connected peers
    fn get_connected_peers(&self) -> Vec<String>;

    /// get number of connected peers
    fn peer_count(&self) -> usize {
        self.get_connected_peers().len()
    }
}

pub struct StarNetworkServer {
    listener: TcpListener,
    connections: Arc<Mutex<HashMap<String, TcpStream>>>,
    address: String,
}

impl StarNetworkServer {
    pub fn new(port: u16) -> Result<Self, NetworkError> {
        let address = format!("0.0.0.0:{}", port);
        let listener = TcpListener::bind(&address).map_err(|e| {
            NetworkError::ConnectionFailed(format!("Failed to bind to {}: {}", address, e))
        })?;

        listener.set_nonblocking(false).map_err(|e| {
            NetworkError::ConnectionFailed(format!("Failed to set blocking mode: {}", e))
        })?;

        Ok(StarNetworkServer {
            listener,
            connections: Arc::new(Mutex::new(HashMap::new())),
            address: address.clone(),
        })
    }

    pub fn address(&self) -> &str {
        &self.address
    }

    pub fn accept_connection(&self) -> Result<(String, TcpStream), NetworkError> {
        let (mut stream, addr) = self
            .listener
            .accept()
            .map_err(|e| NetworkError::ConnectionFailed(format!("Accept failed: {}", e)))?;

        let return_stream = stream.try_clone().map_err(|e| {
            NetworkError::ConnectionFailed(format!("Failed to clone stream: {}", e))
        })?;

        match P2PMessage::receive(&mut stream) {
            Ok(P2PMessage::Join {
                node_id,
                address: _,
                timestamp: _,
            }) => {
                println!("Peer joined: {} ({})", node_id, addr);
                Ok((node_id, return_stream))
            }
            Ok(_) => Err(NetworkError::InvalidMessage(
                "First message must be Join".to_string(),
            )),
            Err(e) => Err(NetworkError::ReceiveFailed(format!(
                "Failed to receive Join message: {}",
                e
            ))),
        }
    }

    pub fn register_peer(&self, node_id: String, stream: TcpStream) {
        let mut connections = self.connections.lock();
        connections.insert(node_id, stream);
    }

    pub fn remove_peer(&self, node_id: &str) {
        let mut connections = self.connections.lock();
        if connections.remove(node_id).is_some() {
            println!("Peer disconnected: {}", node_id);
        }
    }
}

impl NetworkLayer for StarNetworkServer {
    fn broadcast(&self, message: &P2PMessage) -> Result<(), NetworkError> {
        let mut connections = self.connections.lock();
        let mut failed_peers = Vec::new();

        for (node_id, stream) in connections.iter_mut() {
            if let Err(e) = message.send(stream) {
                eprintln!("Failed to send to {}: {}", node_id, e);
                failed_peers.push(node_id.clone());
            }
        }

        for node_id in failed_peers {
            connections.remove(&node_id);
        }

        Ok(())
    }

    fn send_to(&self, node_id: &str, message: &P2PMessage) -> Result<(), NetworkError> {
        let mut connections = self.connections.lock();

        match connections.get_mut(node_id) {
            Some(stream) => message.send(stream).map_err(|e| {
                NetworkError::SendFailed(format!("Failed to send to {}: {}", node_id, e))
            }),
            None => Err(NetworkError::PeerNotFound(node_id.to_string())),
        }
    }

    fn get_connected_peers(&self) -> Vec<String> {
        let connections = self.connections.lock();
        connections.keys().cloned().collect()
    }
}

pub struct StarNetworkClient {
    stream: Arc<Mutex<TcpStream>>,
    bootstrap_address: String,
    node_id: String,
}

impl StarNetworkClient {
    pub fn connect(bootstrap_address: &str, node_id: String) -> Result<Self, NetworkError> {
        let stream = TcpStream::connect(bootstrap_address).map_err(|e| {
            NetworkError::ConnectionFailed(format!(
                "Failed to connect to {}: {}",
                bootstrap_address, e
            ))
        })?;

        let client = StarNetworkClient {
            stream: Arc::new(Mutex::new(stream)),
            bootstrap_address: bootstrap_address.to_string(),
            node_id: node_id.clone(),
        };

        let join_msg = P2PMessage::Join {
            node_id,
            address: "unknown".to_string(),
            timestamp: chrono::Utc::now().timestamp() as u64,
        };

        client.send(&join_msg)?;

        Ok(client)
    }

    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    pub fn bootstrap_address(&self) -> &str {
        &self.bootstrap_address
    }

    pub fn send(&self, message: &P2PMessage) -> Result<(), NetworkError> {
        let mut stream = self.stream.lock();
        message
            .send(&mut *stream)
            .map_err(|e| NetworkError::SendFailed(format!("Failed to send to bootstrap: {}", e)))
    }

    pub fn receive(&self) -> Result<P2PMessage, NetworkError> {
        let mut stream = self.stream.lock();
        P2PMessage::receive(&mut *stream).map_err(|e| {
            NetworkError::ReceiveFailed(format!("Failed to receive from bootstrap: {}", e))
        })
    }

    pub fn is_connected(&self) -> bool {
        let stream = self.stream.lock();
        stream.peer_addr().is_ok()
    }
}
