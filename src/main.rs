use clap::{Parser, Subcommand};
use std::error::Error;

mod message;
mod server;
mod client;
mod ui;
mod file_transfer;

#[derive(Parser)]
#[command(name = "terminal-chat")]
#[command(about = "A real-time terminal chat application")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start a chat server
    Server {
        /// Port to listen on
        #[arg(short, long, default_value = "8080")]
        port: u16,
    },
    /// Connect to a chat server
    Client {
        /// Server address to connect to
        #[arg(short, long, default_value = "127.0.0.1")]
        address: String,
        /// Server port to connect to
        #[arg(short, long, default_value = "8080")]
        port: u16,
        /// Your username
        #[arg(short, long)]
        username: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Server { port } => {
            println!("Starting server on port {}", port);
            server::start_server(port).await?;
        }
        Commands::Client { address, port, username } => {
            println!("Connecting to {}:{} as {}", address, port, username);
            client::start_client(&address, port, &username).await?;
        }
    }

    Ok(())
}
