use crate::p2p::error::NetworkError;
use crate::p2p::gossip::{MessageTracker, compute_message_id};
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

    pub fn accept_connection(&self) -> Result<(String, String, TcpStream), NetworkError> {
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
                address,
                timestamp: _,
            }) => {
                println!("Peer joined: {} ({}) - addr: {}", node_id, addr, address);
                Ok((node_id, address, return_stream))
            }
            Ok(P2PMessage::Heartbeat {
                node_id,
                timestamp: _,
            }) => Ok((
                node_id.clone(),
                format!("heartbeat:{}", node_id),
                return_stream,
            )),
            Ok(P2PMessage::Disconnect { node_id }) => Ok((
                node_id.clone(),
                format!("disconnect:{}", node_id),
                return_stream,
            )),
            Ok(_) => Err(NetworkError::InvalidMessage(
                "First message must be Join or Heartbeat".to_string(),
            )),
            Err(e) => Err(NetworkError::ReceiveFailed(format!(
                "Failed to receive Join/Heartbeat message: {}",
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
    read_stream: Arc<Mutex<TcpStream>>,
    write_stream: Arc<Mutex<TcpStream>>,
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

        let mut read_stream = stream.try_clone().map_err(|e| {
            NetworkError::ConnectionFailed(format!("Failed to clone stream for reading: {}", e))
        })?;

        let mut write_stream = stream;

        read_stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .map_err(|e| {
                NetworkError::ConnectionFailed(format!(
                    "Failed to set read timeout on read_stream: {}",
                    e
                ))
            })?;

        write_stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .map_err(|e| {
                NetworkError::ConnectionFailed(format!(
                    "Failed to set read timeout on write_stream: {}",
                    e
                ))
            })?;

        let client = StarNetworkClient {
            read_stream: Arc::new(Mutex::new(read_stream)),
            write_stream: Arc::new(Mutex::new(write_stream)),
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
        let mut stream = self.write_stream.lock();
        message
            .send(&mut *stream)
            .map_err(|e| NetworkError::SendFailed(format!("Failed to send to bootstrap: {}", e)))
    }

    pub fn receive(&self) -> Result<P2PMessage, NetworkError> {
        let mut stream = self.read_stream.lock();
        P2PMessage::receive(&mut *stream).map_err(|e| {
            NetworkError::ReceiveFailed(format!("Failed to receive from bootstrap: {}", e))
        })
    }

    pub fn is_connected(&self) -> bool {
        let stream = self.read_stream.lock();
        stream.peer_addr().is_ok()
    }
}

// MESH

pub(crate) struct PeerConnection {
    pub(crate) stream: TcpStream,
    pub(crate) connected_at: std::time::Instant,
}

pub struct MeshNetwork {
    node_id: String,
    pub(crate) peers: Arc<Mutex<HashMap<String, PeerConnection>>>,
    message_tracker: MessageTracker,
    max_peers: usize,
}

impl MeshNetwork {
    pub fn new(node_id: String, max_peers: usize) -> Self {
        MeshNetwork {
            node_id,
            peers: Arc::new(Mutex::new(HashMap::new())),
            message_tracker: MessageTracker::new(300),
            max_peers,
        }
    }

    pub fn connect_to_peer(&self, peer_address: &str, peer_id: String) -> Result<(), NetworkError> {
        if self.peers.lock().contains_key(&peer_id) {
            return Ok(());
        }

        if peer_id == self.node_id {
            return Ok(());
        }

        let mut stream = TcpStream::connect(peer_address).map_err(|e| {
            NetworkError::ConnectionFailed(format!("Failed to connect to {}: {}", peer_address, e))
        })?;

        let join = P2PMessage::Join {
            node_id: self.node_id.clone(),
            address: peer_address.to_string(),
            timestamp: chrono::Utc::now().timestamp() as u64,
        };
        join.send(&mut stream)
            .map_err(|e| NetworkError::SendFailed(format!("Failed to send Join: {}", e)))?;

        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .map_err(|e| NetworkError::ConnectionFailed(format!("Failed to set timeout: {}", e)))?;

        match P2PMessage::receive(&mut stream) {
            Ok(P2PMessage::Rejected { reason }) => {
                println!("Connection rejected: {}", reason);
                return Err(NetworkError::ConnectionFailed(format!(
                    "Rejected by peer: {}",
                    reason
                )));
            }
            Err(e)
                if e.to_string().contains("timed out")
                    || e.to_string().contains("WouldBlock")
                    || e.to_string().contains("10060") =>
            {
                println!("Connection accepted");
            }
            Err(e)
                if e.to_string().contains("UnexpectedEof")
                    || e.to_string().contains("failed to fill whole buffer") =>
            {
                println!("Connection closed by peer (rejected)");
                return Err(NetworkError::ConnectionFailed(
                    "Connection rejected".to_string(),
                ));
            }
            Err(e) => {
                println!("Error reading after Join: {}", e);
            }
            Ok(msg) => {
                println!(
                    "Received message after Join: {:?} - connection accepted",
                    msg
                );
            }
        }

        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(1)))
            .map_err(|e| NetworkError::ConnectionFailed(format!("Failed to set timeout: {}", e)))?;

        self.peers.lock().insert(
            peer_id.clone(),
            PeerConnection {
                stream,
                connected_at: std::time::Instant::now(),
            },
        );
        println!("Connected to peer: {}", peer_id);

        Ok(())
    }

    pub fn accept_peer(&self, peer_id: String, stream: TcpStream) -> bool {
        let mut peers = self.peers.lock();

        if peers.contains_key(&peer_id) {
            if self.node_id < peer_id {
                drop(stream);
                println!("Duplicate connection to {} - keeping outgoing", peer_id);
                return false;
            } else {
                peers.remove(&peer_id);
                println!("Duplicate connection to {} - keeping incoming", peer_id);
            }
        }

        peers.insert(
            peer_id.clone(),
            PeerConnection {
                stream,
                connected_at: std::time::Instant::now(),
            },
        );
        println!("Peer connected: {}", peer_id);
        true
    }

    pub fn remove_peer(&self, peer_id: &str) {
        self.peers.lock().remove(peer_id);
        println!("Peer disconnected: {}", peer_id);
    }

    pub fn gossip_broadcast(
        &self,
        message: &P2PMessage,
        exclude_peer: Option<&str>,
    ) -> Result<(), NetworkError> {
        let msg_id = compute_message_id(message);

        if self.message_tracker.is_seen(&msg_id) {
            return Ok(());
        }

        self.message_tracker.mark_seen(msg_id);

        let mut peers = self.peers.lock();
        let mut failed_peers = Vec::new();

        for (peer_id, conn) in peers.iter_mut() {
            if let Some(exclude) = exclude_peer {
                if peer_id == exclude {
                    continue;
                }
            }

            if let Err(e) = message.send(&mut conn.stream) {
                eprintln!("Failed to gossip to {}: {}", peer_id, e);
                failed_peers.push(peer_id.clone());
            }
        }

        for peer_id in failed_peers {
            peers.remove(&peer_id);
        }

        Ok(())
    }

    pub fn peer_count(&self) -> usize {
        self.peers.lock().len()
    }

    pub fn peer_count_limit(&self) -> usize {
        self.max_peers
    }

    pub fn cleanup_tracker(&self) {
        self.message_tracker.cleanup();
    }
}

impl NetworkLayer for MeshNetwork {
    fn broadcast(&self, message: &P2PMessage) -> Result<(), NetworkError> {
        self.gossip_broadcast(message, None)
    }

    fn send_to(&self, node_id: &str, message: &P2PMessage) -> Result<(), NetworkError> {
        let mut peers = self.peers.lock();
        if let Some(conn) = peers.get_mut(node_id) {
            message.send(&mut conn.stream).map_err(|e| {
                NetworkError::SendFailed(format!("Failed to send to {}: {}", node_id, e))
            })?;
        } else {
            return Err(NetworkError::PeerNotFound(node_id.to_string()));
        }
        Ok(())
    }

    fn get_connected_peers(&self) -> Vec<String> {
        self.peers.lock().keys().cloned().collect()
    }
}
