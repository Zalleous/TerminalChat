use crate::message::Message;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::error::Error;
use std::io;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;

pub struct ChatUI {
    username: String,
    messages: Vec<String>,
    input: String,
    message_sender: mpsc::UnboundedSender<String>,
    message_receiver: mpsc::UnboundedReceiver<Message>,
    ui_sender: mpsc::UnboundedSender<Message>,
}

impl ChatUI {
    pub fn new(
        username: String,
        message_sender: mpsc::UnboundedSender<String>,
    ) -> Result<Self, Box<dyn Error>> {
        let (ui_sender, message_receiver) = mpsc::unbounded_channel();
        
        Ok(ChatUI {
            username,
            messages: Vec::new(),
            input: String::new(),
            message_sender,
            message_receiver,
            ui_sender,
        })
    }

    pub fn get_sender(&self) -> mpsc::UnboundedSender<Message> {
        self.ui_sender.clone()
    }

    pub async fn run(&mut self) -> Result<(), Box<dyn Error>> {
        // Setup terminal
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;

        let result = self.run_app().await;

        // Restore terminal
        disable_raw_mode()?;
        execute!(stdout, LeaveAlternateScreen, DisableMouseCapture)?;

        result
    }

    async fn run_app(&mut self) -> Result<(), Box<dyn Error>> {
        loop {
            self.draw()?;

            // Handle events with timeout
            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        match key.code {
                            KeyCode::Char('q') if key.modifiers.contains(event::KeyModifiers::CONTROL) => {
                                break;
                            }
                            KeyCode::Enter => {
                                if !self.input.trim().is_empty() {
                                    let text = self.input.clone();
                                    self.input.clear();
                                    
                                    // Check if it's a file command
                                    if text.starts_with("/file ") {
                                        self.handle_file_command(&text[6..]).await?;
                                    } else {
                                        // Send regular message
                                        let _ = self.message_sender.send(text);
                                    }
                                }
                            }
                            KeyCode::Char(c) => {
                                self.input.push(c);
                            }
                            KeyCode::Backspace => {
                                self.input.pop();
                            }
                            _ => {}
                        }
                    }
                }
            }

            // Handle incoming messages
            while let Ok(msg) = self.message_receiver.try_recv() {
                self.add_message(msg);
            }
        }

        Ok(())
    }

    fn draw(&self) -> Result<(), Box<dyn Error>> {
        let (width, height) = crossterm::terminal::size()?;
        
        // Clear screen
        execute!(io::stdout(), crossterm::terminal::Clear(crossterm::terminal::ClearType::All))?;
        execute!(io::stdout(), crossterm::cursor::MoveTo(0, 0))?;

        // Draw title
        let title = format!("Terminal Chat - {} (Ctrl+Q to quit, /file <path> to send file)", self.username);
        print!("{}", title);
        execute!(io::stdout(), crossterm::cursor::MoveTo(0, 1))?;
        print!("{}", "=".repeat(width as usize));

        // Draw messages (leave space for title, separator, input separator, and input)
        let message_height = height.saturating_sub(4) as usize;
        let start_idx = if self.messages.len() > message_height {
            self.messages.len() - message_height
        } else {
            0
        };

        for (i, msg) in self.messages[start_idx..].iter().enumerate() {
            execute!(io::stdout(), crossterm::cursor::MoveTo(0, (i + 2) as u16))?;
            print!("{}", msg);
        }

        // Move to input separator line
        execute!(io::stdout(), crossterm::cursor::MoveTo(0, height - 2))?;
        print!("{}", "=".repeat(width as usize));
        
        // Move to input line
        execute!(io::stdout(), crossterm::cursor::MoveTo(0, height - 1))?;
        print!("> {}", self.input);
        
        // Position cursor at end of input
        execute!(io::stdout(), crossterm::cursor::MoveTo((self.input.len() + 2) as u16, height - 1))?;
        
        io::stdout().flush()?;
        Ok(())
    }

    fn add_message(&mut self, msg: Message) {
        let formatted = match msg {
            Message::Text { username, content, timestamp } => {
                format!("[{}] {}: {}", self.format_time(timestamp), username, content)
            }
            Message::File { username, filename, size, timestamp, .. } => {
                format!("[{}] {} shared file: {} ({} bytes)", 
                    self.format_time(timestamp), username, filename, size)
            }
            Message::UserJoined { username, timestamp } => {
                format!("[{}] * {} joined the chat", self.format_time(timestamp), username)
            }
            Message::UserLeft { username, timestamp } => {
                format!("[{}] * {} left the chat", self.format_time(timestamp), username)
            }
            Message::System { content, timestamp } => {
                format!("[{}] * {}", self.format_time(timestamp), content)
            }
        };
        
        self.messages.push(formatted);
    }

    fn format_time(&self, time: SystemTime) -> String {
        if let Ok(duration) = time.duration_since(UNIX_EPOCH) {
            let secs = duration.as_secs();
            let hours = (secs / 3600) % 24;
            let minutes = (secs / 60) % 60;
            let seconds = secs % 60;
            format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
        } else {
            "??:??:??".to_string()
        }
    }

    async fn handle_file_command(&mut self, filepath: &str) -> Result<(), Box<dyn Error>> {
        use crate::file_transfer::FileTransfer;
        
        match FileTransfer::read_file(filepath) {
            Ok(file_msg) => {
                // Send the file message through the message sender
                if let Ok(json) = file_msg.to_json() {
                    let _ = self.message_sender.send(format!("FILE:{}", json));
                }
                self.messages.push(format!("Sending file: {}", filepath));
            }
            Err(e) => {
                self.messages.push(format!("Error reading file {}: {}", filepath, e));
            }
        }
        Ok(())
    }
}

use std::io::Write;
