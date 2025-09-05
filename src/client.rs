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
    let stream = TcpStream::connect(format!("{}:{}", address, port)).await?;
    
    // Split the stream for reading and writing
    let (reader, mut writer) = stream.into_split();
    
    // Send username as first message
    writer.write_all(format!("{}\n", username).as_bytes()).await?;

    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    let mut ui = ChatUI::new(username.to_string(), tx)?;

    // Create a buffered reader
    let mut reader = BufReader::new(reader);

    // Handle incoming messages from server
    let ui_tx = ui.get_sender();
    tokio::spawn(async move {
        let mut line = String::new();
        while reader.read_line(&mut line).await.unwrap_or(0) > 0 {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                if let Ok(msg) = Message::from_json(trimmed) {
                    // Handle file messages and save them
                    if let Message::File { filename, .. } = &msg {
                        use crate::file_transfer::FileTransfer;
                        match FileTransfer::save_file(&msg, "downloads") {
                            Ok(saved_path) => {
                                println!("File saved to: {}", saved_path);
                            }
                            Err(e) => {
                                eprintln!("Error saving file {}: {}", filename, e);
                            }
                        }
                    }
                    let _ = ui_tx.send(msg);
                } else {
                    // If JSON parsing fails, treat as raw text (fallback)
                    eprintln!("Failed to parse message: {}", trimmed);
                }
            }
            line.clear();
        }
    });

    // Handle outgoing messages to server
    tokio::spawn(async move {
        while let Some(text) = rx.recv().await {
            // Send raw text instead of JSON to server
            let _ = writer.write_all(format!("{}\n", text).as_bytes()).await;
        }
    });

    // Run the UI
    ui.run().await?;

    Ok(())
}
