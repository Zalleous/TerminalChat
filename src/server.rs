use crate::message::Message;
use crate::validation;
use crate::config;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, Mutex};
use uuid::Uuid;

type ClientId = Uuid;
type Clients = Arc<Mutex<HashMap<ClientId, ClientInfo>>>;

#[derive(Debug)]
struct ClientInfo {
    username: String,
}

pub async fn start_server(port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    let clients: Clients = Arc::new(Mutex::new(HashMap::new()));
    // Changed to broadcast tuples of (sender_id, message) to filter own messages
    let (broadcast_tx, _) = broadcast::channel::<(Option<ClientId>, String)>(config::BROADCAST_CHANNEL_SIZE);

    // Track active connections for limiting
    let active_connections = Arc::new(AtomicUsize::new(0));

    println!("Server listening on port {} (max {} connections)", port, config::MAX_CONNECTIONS);

    loop {
        let (socket, addr) = listener.accept().await?;

        // Check connection limit
        let current_connections = active_connections.load(Ordering::SeqCst);
        if current_connections >= config::MAX_CONNECTIONS {
            eprintln!("Connection limit reached ({}), rejecting connection from {}", config::MAX_CONNECTIONS, addr);
            drop(socket);
            continue;
        }

        println!("New connection from: {} ({}/{})", addr, current_connections + 1, config::MAX_CONNECTIONS);

        let clients = clients.clone();
        let broadcast_tx = broadcast_tx.clone();
        let active_connections = active_connections.clone();

        // Increment connection counter
        active_connections.fetch_add(1, Ordering::SeqCst);

        tokio::spawn(async move {
            if let Err(e) = handle_client(socket, clients, broadcast_tx).await {
                eprintln!("Error handling client {}: {}", addr, e);
            }
            // Decrement connection counter when client disconnects
            active_connections.fetch_sub(1, Ordering::SeqCst);
            println!("Client {} disconnected, active connections: {}", addr, active_connections.load(Ordering::SeqCst));
        });
    }
}

async fn handle_client(
    socket: TcpStream,
    clients: Clients,
    broadcast_tx: broadcast::Sender<(Option<ClientId>, String)>,
) -> Result<(), Box<dyn std::error::Error>> {
    let client_id = Uuid::new_v4();
    let mut broadcast_rx = broadcast_tx.subscribe();

    // Split the socket for reading and writing
    let (reader, mut writer) = socket.into_split();
    let mut reader = BufReader::new(reader);

    // Read username from first message
    let mut username_line = String::new();
    reader.read_line(&mut username_line).await?;
    let username = username_line.trim().to_string();

    // Validate username
    if let Err(e) = validation::validate_username(&username) {
        eprintln!("Invalid username '{}': {}", username, e);
        return Err(format!("Invalid username: {}", e).into());
    }

    // Add client to the map
    {
        let mut clients_guard = clients.lock().await;
        clients_guard.insert(client_id, ClientInfo {
            username: username.clone(),
        });
    }

    // Broadcast user joined (None as sender means system message, send to all)
    let join_msg = Message::new_user_joined(username.clone());
    if let Err(e) = broadcast_tx.send((None, join_msg.to_json()?)) {
        eprintln!("No receivers for join message: {}", e);
    }

    // Send welcome message
    let welcome_msg = Message::new_system(format!("Welcome to the chat, {}!", username));
    writer.write_all(format!("{}\n", welcome_msg.to_json()?).as_bytes()).await?;

    // Handle incoming messages from this client
    let broadcast_tx_for_reader = broadcast_tx.clone();
    let username_for_reader = username.clone();
    let clients_for_reader = clients.clone();
    let sender_id = client_id;

    tokio::spawn(async move {
        let mut line = String::new();

        loop {
            match reader.read_line(&mut line).await {
                Ok(0) => {
                    println!("Client {} disconnected", username_for_reader);
                    break;
                }
                Ok(bytes_read) => {
                    // Check message size limit
                    if bytes_read > config::MAX_MESSAGE_SIZE {
                        eprintln!("Message from {} exceeds size limit ({} > {})", username_for_reader, bytes_read, config::MAX_MESSAGE_SIZE);
                        line.clear();
                        continue;
                    }

                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        // Check if it's a file message
                        if trimmed.starts_with("FILE:") {
                            // Forward the file message as-is, tagged with sender_id
                            if let Err(e) = broadcast_tx_for_reader.send((Some(sender_id), trimmed[5..].to_string())) {
                                eprintln!("Failed to broadcast file message: {}", e);
                            }
                        } else {
                            // Validate message content
                            if let Err(e) = validation::validate_message(trimmed) {
                                eprintln!("Invalid message from {}: {}", username_for_reader, e);
                                line.clear();
                                continue;
                            }

                            // Create regular text message and send as JSON, tagged with sender_id
                            let msg = Message::new_text(username_for_reader.clone(), trimmed.to_string());
                            match msg.to_json() {
                                Ok(json) => {
                                    if let Err(e) = broadcast_tx_for_reader.send((Some(sender_id), json)) {
                                        eprintln!("Failed to broadcast message: {}", e);
                                    }
                                }
                                Err(e) => {
                                    eprintln!("Failed to serialize message: {}", e);
                                }
                            }
                        }
                    }
                    line.clear();
                }
                Err(e) => {
                    eprintln!("Error reading from client {}: {}", username_for_reader, e);
                    break;
                }
            }
        }

        // Client disconnected (None means send to all)
        let leave_msg = Message::new_user_left(username_for_reader.clone());
        match leave_msg.to_json() {
            Ok(json) => {
                if let Err(e) = broadcast_tx_for_reader.send((None, json)) {
                    eprintln!("Failed to broadcast leave message: {}", e);
                }
            }
            Err(e) => {
                eprintln!("Failed to serialize leave message: {}", e);
            }
        }

        let mut clients_guard = clients_for_reader.lock().await;
        clients_guard.remove(&sender_id);
    });

    // Handle outgoing messages to this client
    loop {
        match broadcast_rx.recv().await {
            Ok((sender_id, json_msg)) => {
                // Filter out messages from this client (don't echo own messages)
                // Unless sender_id is None (system messages, always send)
                if sender_id.is_none() || sender_id != Some(client_id) {
                    if writer.write_all(format!("{}\n", json_msg).as_bytes()).await.is_err() {
                        eprintln!("Failed to write to client {}", username);
                        break;
                    }
                }
            }
            Err(e) => {
                eprintln!("Broadcast receive error for client {}: {}", username, e);
                break;
            }
        }
    }

    Ok(())
}
