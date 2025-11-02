use std::fmt;

/// custom P2P error
#[derive(Debug)]
pub enum NetworkError {
    /// failed to establish connection
    ConnectionFailed(String),

    ///failed to send message
    SendFailed(String),

    /// failed to receive message
    ReceiveFailed(String),

    /// peer with id not found
    PeerNotFound(String),

    /// invalid message
    InvalidMessage(String),

    /// io error
    IoError(std::io::Error),

    /// serialization error
    SerializationError(String),
}

impl fmt::Display for NetworkError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            NetworkError::ConnectionFailed(msg) => write!(f, "Connection failed: {}", msg),
            NetworkError::SendFailed(msg) => write!(f, "Send failed: {}", msg),
            NetworkError::ReceiveFailed(msg) => write!(f, "Receive failed: {}", msg),
            NetworkError::PeerNotFound(id) => write!(f, "Peer not found: {}", id),
            NetworkError::InvalidMessage(msg) => write!(f, "Invalid message: {}", msg),
            NetworkError::IoError(err) => write!(f, "IO error: {}", err),
            NetworkError::SerializationError(msg) => write!(f, "Serialization error: {}", msg),
        }
    }
}

impl std::error::Error for NetworkError {}

impl From<std::io::Error> for NetworkError {
    fn from(err: std::io::Error) -> Self {
        NetworkError::IoError(err)
    }
}

impl From<Box<dyn std::error::Error>> for NetworkError {
    fn from(err: Box<dyn std::error::Error>) -> Self {
        NetworkError::SerializationError(err.to_string())
    }
}
