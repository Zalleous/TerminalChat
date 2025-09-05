use crate::message::Message;
use crate::ui::ChatUI;
use std::error::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::mpsc;

pub async fn start_client(
    address: &str,
    port: u16,
    username: &str,
) -> Result<(), Box<dyn Error>> {
    let mut stream = TcpStream::connect(format!("{}:{}", address, port)).await?;
    
    // Send username as first message
    stream.write_all(format!("{}\n", username).as_bytes()).await?;

    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    let mut ui = ChatUI::new(username.to_string(), tx)?;

    // Clone stream for reading
    let mut reader = BufReader::new(&mut stream);

    // Handle incoming messages from server
    let ui_tx = ui.get_sender();
    tokio::spawn(async move {
        let mut line = String::new();
        while reader.read_line(&mut line).await.unwrap_or(0) > 0 {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                if let Ok(msg) = Message::from_json(trimmed) {
                    let _ = ui_tx.send(msg);
                }
            }
            line.clear();
        }
    });

    // Handle outgoing messages to server
    tokio::spawn(async move {
        while let Some(text) = rx.recv().await {
            let msg = Message::new_text(username.to_string(), text);
            if let Ok(json) = msg.to_json() {
                let _ = stream.write_all(format!("{}\n", json).as_bytes()).await;
            }
        }
    });

    // Run the UI
    ui.run().await?;

    Ok(())
}
