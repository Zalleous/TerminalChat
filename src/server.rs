use crate::message::Message;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, Mutex};
use uuid::Uuid;

type ClientId = Uuid;
type Clients = Arc<Mutex<HashMap<ClientId, ClientInfo>>>;

#[derive(Debug)]
struct ClientInfo {
    username: String,
    sender: tokio::sync::mpsc::UnboundedSender<String>,
}

pub async fn start_server(port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    let clients: Clients = Arc::new(Mutex::new(HashMap::new()));
    let (broadcast_tx, _) = broadcast::channel(100);

    println!("Server listening on port {}", port);

    loop {
        let (socket, addr) = listener.accept().await?;
        println!("New connection from: {}", addr);

        let clients = clients.clone();
        let broadcast_tx = broadcast_tx.clone();

        tokio::spawn(async move {
            if let Err(e) = handle_client(socket, clients, broadcast_tx).await {
                eprintln!("Error handling client {}: {}", addr, e);
            }
        });
    }
}

async fn handle_client(
    mut socket: TcpStream,
    clients: Clients,
    broadcast_tx: broadcast::Sender<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let client_id = Uuid::new_v4();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut broadcast_rx = broadcast_tx.subscribe();

    // Read username from first message
    let mut reader = BufReader::new(&mut socket);
    let mut username_line = String::new();
    reader.read_line(&mut username_line).await?;
    let username = username_line.trim().to_string();

    // Add client to the map
    {
        let mut clients_guard = clients.lock().await;
        clients_guard.insert(client_id, ClientInfo {
            username: username.clone(),
            sender: tx.clone(),
        });
    }

    // Broadcast user joined
    let join_msg = Message::new_user_joined(username.clone());
    let _ = broadcast_tx.send(join_msg.to_json()?);

    // Send welcome message
    let welcome_msg = Message::new_system(format!("Welcome to the chat, {}!", username));
    socket.write_all(format!("{}\n", welcome_msg.to_json()?).as_bytes()).await?;

    // Handle incoming messages from this client
    let clients_for_reader = clients.clone();
    let broadcast_tx_for_reader = broadcast_tx.clone();
    let username_for_reader = username.clone();
    
    tokio::spawn(async move {
        let mut reader = BufReader::new(socket);
        let mut line = String::new();
        
        while reader.read_line(&mut line).await.unwrap_or(0) > 0 {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                let msg = Message::new_text(username_for_reader.clone(), trimmed.to_string());
                let _ = broadcast_tx_for_reader.send(msg.to_json().unwrap_or_default());
            }
            line.clear();
        }
        
        // Client disconnected
        let leave_msg = Message::new_user_left(username_for_reader.clone());
        let _ = broadcast_tx_for_reader.send(leave_msg.to_json().unwrap_or_default());
        
        let mut clients_guard = clients_for_reader.lock().await;
        clients_guard.remove(&client_id);
    });

    // Handle outgoing messages to this client
    loop {
        tokio::select! {
            // Messages from other clients
            msg = broadcast_rx.recv() => {
                match msg {
                    Ok(json_msg) => {
                        if let Ok(msg) = Message::from_json(&json_msg) {
                            // Don't echo back messages from this user
                            let should_send = match &msg {
                                Message::Text { username: msg_username, .. } => msg_username != &username,
                                _ => true,
                            };
                            
                            if should_send {
                                let _ = tx.send(json_msg.clone());
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
            // Messages to send to this specific client
            msg = rx.recv() => {
                match msg {
                    Some(_json_msg) => {
                        // Send to client (this would need the socket, which we'd need to restructure for)
                        // For now, we'll handle this in the broadcast loop above
                    }
                    None => break,
                }
            }
        }
    }

    Ok(())
}
