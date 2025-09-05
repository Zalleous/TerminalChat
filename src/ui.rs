use crate::message::Message;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, MouseEvent, MouseEventKind, MouseButton},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::error::Error;
use std::io;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::path::Path;
use std::fs;
use tokio::sync::mpsc;
use arboard::Clipboard;
use glob::glob;

#[derive(Clone)]
pub struct FileInfo {
    pub filename: String,
    pub size: u64,
    pub data: Vec<u8>,
    pub sender: String,
}

pub struct ChatUI {
    username: String,
    messages: Vec<String>,
    input: String,
    message_sender: mpsc::UnboundedSender<String>,
    message_receiver: mpsc::UnboundedReceiver<Message>,
    ui_sender: mpsc::UnboundedSender<Message>,
    // Selection state
    selection_start: Option<(usize, usize)>, // (message_index, char_index)
    selection_end: Option<(usize, usize)>,
    selecting: bool,
    // File management
    received_files: Vec<FileInfo>,
    // Tab completion
    completion_candidates: Vec<String>,
    completion_index: usize,
    last_tab_input: String,
    // UI state
    mode: UIMode,
    file_viewer_index: Option<usize>,
    scroll_offset: usize,
}

#[derive(PartialEq)]
enum UIMode {
    Chat,
    FileViewer,
    FileList,
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
            selection_start: None,
            selection_end: None,
            selecting: false,
            received_files: Vec::new(),
            completion_candidates: Vec::new(),
            completion_index: 0,
            last_tab_input: String::new(),
            mode: UIMode::Chat,
            file_viewer_index: None,
            scroll_offset: 0,
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
                match event::read()? {
                    Event::Key(key) if key.kind == KeyEventKind::Press => {
                        match self.mode {
                            UIMode::Chat => self.handle_chat_key(key).await?,
                            UIMode::FileViewer => self.handle_file_viewer_key(key)?,
                            UIMode::FileList => self.handle_file_list_key(key)?,
                        }
                    }
                    Event::Mouse(mouse) => {
                        if self.mode == UIMode::Chat {
                            self.handle_mouse_event(mouse)?;
                        }
                    }
                    _ => {}
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
        match self.mode {
            UIMode::Chat => self.draw_chat()?,
            UIMode::FileViewer => self.draw_file_viewer()?,
            UIMode::FileList => self.draw_file_list()?,
        }
        Ok(())
    }

    fn draw_chat(&self) -> Result<(), Box<dyn Error>> {
        let (width, height) = crossterm::terminal::size()?;
        
        // Clear screen
        execute!(io::stdout(), crossterm::terminal::Clear(crossterm::terminal::ClearType::All))?;
        execute!(io::stdout(), crossterm::cursor::MoveTo(0, 0))?;

        // Draw title
        let title = format!("Terminal Chat - {} (Ctrl+Q: quit, /file <path>: send, F1: files, Ctrl+C: copy)", self.username);
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
            
            // Highlight selected text
            if let (Some(start), Some(end)) = (self.selection_start, self.selection_end) {
                let msg_idx = start_idx + i;
                if msg_idx >= start.0 && msg_idx <= end.0 {
                    self.print_with_selection(msg, msg_idx, start, end)?;
                } else {
                    print!("{}", msg);
                }
            } else {
                print!("{}", msg);
            }
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

    fn draw_file_list(&self) -> Result<(), Box<dyn Error>> {
        let (width, height) = crossterm::terminal::size()?;
        
        execute!(io::stdout(), crossterm::terminal::Clear(crossterm::terminal::ClearType::All))?;
        execute!(io::stdout(), crossterm::cursor::MoveTo(0, 0))?;

        print!("Received Files (ESC: back, Enter: view, D: download)");
        execute!(io::stdout(), crossterm::cursor::MoveTo(0, 1))?;
        print!("{}", "=".repeat(width as usize));

        for (i, file) in self.received_files.iter().enumerate() {
            execute!(io::stdout(), crossterm::cursor::MoveTo(0, (i + 2) as u16))?;
            print!("{}. {} ({} bytes) from {}", i + 1, file.filename, file.size, file.sender);
        }

        if self.received_files.is_empty() {
            execute!(io::stdout(), crossterm::cursor::MoveTo(0, 2))?;
            print!("No files received yet.");
        }

        io::stdout().flush()?;
        Ok(())
    }

    fn draw_file_viewer(&self) -> Result<(), Box<dyn Error>> {
        let (width, height) = crossterm::terminal::size()?;
        
        execute!(io::stdout(), crossterm::terminal::Clear(crossterm::terminal::ClearType::All))?;
        execute!(io::stdout(), crossterm::cursor::MoveTo(0, 0))?;

        if let Some(index) = self.file_viewer_index {
            if let Some(file) = self.received_files.get(index) {
                print!("File: {} ({} bytes) - ESC: back, D: download", file.filename, file.size);
                execute!(io::stdout(), crossterm::cursor::MoveTo(0, 1))?;
                print!("{}", "=".repeat(width as usize));

                // Try to display file content as text
                let content = String::from_utf8_lossy(&file.data);
                let lines: Vec<&str> = content.lines().collect();
                let display_height = height.saturating_sub(3) as usize;
                
                let start_line = self.scroll_offset;
                let end_line = (start_line + display_height).min(lines.len());

                for (i, line) in lines[start_line..end_line].iter().enumerate() {
                    execute!(io::stdout(), crossterm::cursor::MoveTo(0, (i + 2) as u16))?;
                    print!("{}", line);
                }

                if lines.len() > display_height {
                    execute!(io::stdout(), crossterm::cursor::MoveTo(0, height - 1))?;
                    print!("Scroll: ↑/↓ arrows | Line {}/{}", start_line + 1, lines.len());
                }
            }
        }

        io::stdout().flush()?;
        Ok(())
    }

    fn print_with_selection(&self, msg: &str, msg_idx: usize, start: (usize, usize), end: (usize, usize)) -> Result<(), Box<dyn Error>> {
        if msg_idx == start.0 && msg_idx == end.0 {
            // Selection within single message
            let before = &msg[..start.1.min(msg.len())];
            let selected = &msg[start.1.min(msg.len())..end.1.min(msg.len())];
            let after = &msg[end.1.min(msg.len())..];
            
            print!("{}", before);
            print!("\x1b[7m{}\x1b[0m", selected); // Reverse video for selection
            print!("{}", after);
        } else if msg_idx == start.0 {
            // Start of multi-line selection
            let before = &msg[..start.1.min(msg.len())];
            let selected = &msg[start.1.min(msg.len())..];
            
            print!("{}", before);
            print!("\x1b[7m{}\x1b[0m", selected);
        } else if msg_idx == end.0 {
            // End of multi-line selection
            let selected = &msg[..end.1.min(msg.len())];
            let after = &msg[end.1.min(msg.len())..];
            
            print!("\x1b[7m{}\x1b[0m", selected);
            print!("{}", after);
        } else {
            // Middle of multi-line selection
            print!("\x1b[7m{}\x1b[0m", msg);
        }
        Ok(())
    }

    async fn handle_chat_key(&mut self, key: crossterm::event::KeyEvent) -> Result<(), Box<dyn Error>> {
        match key.code {
            KeyCode::Char('q') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                std::process::exit(0);
            }
            KeyCode::Char('c') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                self.copy_selection()?;
            }
            KeyCode::F(1) => {
                self.mode = UIMode::FileList;
            }
            KeyCode::Enter => {
                if !self.input.trim().is_empty() {
                    let text = self.input.clone();
                    self.input.clear();
                    self.completion_candidates.clear();
                    
                    // Check if it's a file command
                    if text.starts_with("/file ") {
                        self.handle_file_command(&text[6..]).await?;
                    } else {
                        // Send regular message
                        let _ = self.message_sender.send(text);
                    }
                }
            }
            KeyCode::Tab => {
                self.handle_tab_completion()?;
            }
            KeyCode::Char(c) => {
                self.input.push(c);
                self.completion_candidates.clear();
            }
            KeyCode::Backspace => {
                self.input.pop();
                self.completion_candidates.clear();
            }
            KeyCode::Esc => {
                self.clear_selection();
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_file_list_key(&mut self, key: crossterm::event::KeyEvent) -> Result<(), Box<dyn Error>> {
        match key.code {
            KeyCode::Esc => {
                self.mode = UIMode::Chat;
            }
            KeyCode::Enter => {
                if !self.received_files.is_empty() {
                    self.file_viewer_index = Some(0);
                    self.mode = UIMode::FileViewer;
                    self.scroll_offset = 0;
                }
            }
            KeyCode::Char(c) if c.is_ascii_digit() => {
                let index = c.to_digit(10).unwrap() as usize;
                if index > 0 && index <= self.received_files.len() {
                    self.file_viewer_index = Some(index - 1);
                    self.mode = UIMode::FileViewer;
                    self.scroll_offset = 0;
                }
            }
            KeyCode::Char('d') | KeyCode::Char('D') => {
                self.download_all_files()?;
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_file_viewer_key(&mut self, key: crossterm::event::KeyEvent) -> Result<(), Box<dyn Error>> {
        match key.code {
            KeyCode::Esc => {
                self.mode = UIMode::FileList;
            }
            KeyCode::Up => {
                if self.scroll_offset > 0 {
                    self.scroll_offset -= 1;
                }
            }
            KeyCode::Down => {
                self.scroll_offset += 1;
            }
            KeyCode::Char('d') | KeyCode::Char('D') => {
                if let Some(index) = self.file_viewer_index {
                    self.download_file(index)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_mouse_event(&mut self, mouse: MouseEvent) -> Result<(), Box<dyn Error>> {
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.start_selection(mouse.column, mouse.row);
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                self.update_selection(mouse.column, mouse.row);
            }
            MouseEventKind::Up(MouseButton::Left) => {
                self.end_selection();
            }
            _ => {}
        }
        Ok(())
    }

    fn start_selection(&mut self, x: u16, y: u16) {
        if y >= 2 {
            let msg_index = (y - 2) as usize;
            let char_index = x as usize;
            self.selection_start = Some((msg_index, char_index));
            self.selecting = true;
        }
    }

    fn update_selection(&mut self, x: u16, y: u16) {
        if self.selecting && y >= 2 {
            let msg_index = (y - 2) as usize;
            let char_index = x as usize;
            self.selection_end = Some((msg_index, char_index));
        }
    }

    fn end_selection(&mut self) {
        self.selecting = false;
    }

    fn clear_selection(&mut self) {
        self.selection_start = None;
        self.selection_end = None;
        self.selecting = false;
    }

    fn copy_selection(&mut self) -> Result<(), Box<dyn Error>> {
        if let (Some(start), Some(end)) = (self.selection_start, self.selection_end) {
            let mut selected_text = String::new();
            
            let (start, end) = if start.0 > end.0 || (start.0 == end.0 && start.1 > end.1) {
                (end, start)
            } else {
                (start, end)
            };

            for msg_idx in start.0..=end.0.min(self.messages.len().saturating_sub(1)) {
                if let Some(msg) = self.messages.get(msg_idx) {
                    if start.0 == end.0 {
                        // Single line selection
                        let start_char = start.1.min(msg.len());
                        let end_char = end.1.min(msg.len());
                        selected_text.push_str(&msg[start_char..end_char]);
                    } else if msg_idx == start.0 {
                        // First line
                        let start_char = start.1.min(msg.len());
                        selected_text.push_str(&msg[start_char..]);
                        selected_text.push('\n');
                    } else if msg_idx == end.0 {
                        // Last line
                        let end_char = end.1.min(msg.len());
                        selected_text.push_str(&msg[..end_char]);
                    } else {
                        // Middle lines
                        selected_text.push_str(msg);
                        selected_text.push('\n');
                    }
                }
            }

            if !selected_text.is_empty() {
                let mut clipboard = Clipboard::new()?;
                clipboard.set_text(selected_text)?;
                self.messages.push("* Text copied to clipboard".to_string());
            }
        }
        Ok(())
    }

    fn handle_tab_completion(&mut self) -> Result<(), Box<dyn Error>> {
        if self.input.starts_with("/file ") {
            let path_part = &self.input[6..];
            
            // If this is a new tab completion or the input changed
            if self.completion_candidates.is_empty() || self.last_tab_input != path_part {
                self.completion_candidates = self.get_file_completions(path_part)?;
                self.completion_index = 0;
                self.last_tab_input = path_part.to_string();
            } else {
                // Cycle through candidates
                self.completion_index = (self.completion_index + 1) % self.completion_candidates.len().max(1);
            }

            if !self.completion_candidates.is_empty() {
                let completion = &self.completion_candidates[self.completion_index];
                self.input = format!("/file {}", completion);
            }
        }
        Ok(())
    }

    fn get_file_completions(&self, partial_path: &str) -> Result<Vec<String>, Box<dyn Error>> {
        let mut completions = Vec::new();
        
        let search_pattern = if partial_path.is_empty() {
            "*".to_string()
        } else if partial_path.ends_with('/') {
            format!("{}*", partial_path)
        } else {
            format!("{}*", partial_path)
        };

        for entry in glob(&search_pattern)? {
            if let Ok(path) = entry {
                if let Some(path_str) = path.to_str() {
                    completions.push(path_str.to_string());
                }
            }
        }

        // Also try in current directory if no path separator
        if !partial_path.contains('/') && !partial_path.contains('\\') {
            let current_dir_pattern = format!("./{}", search_pattern);
            for entry in glob(&current_dir_pattern)? {
                if let Ok(path) = entry {
                    if let Some(path_str) = path.to_str() {
                        if let Some(filename) = path_str.strip_prefix("./") {
                            completions.push(filename.to_string());
                        }
                    }
                }
            }
        }

        completions.sort();
        completions.dedup();
        Ok(completions)
    }

    fn download_file(&mut self, index: usize) -> Result<(), Box<dyn Error>> {
        if let Some(file) = self.received_files.get(index) {
            use crate::file_transfer::FileTransfer;
            let msg = Message::File {
                username: file.sender.clone(),
                filename: file.filename.clone(),
                size: file.size,
                data: file.data.clone(),
                timestamp: SystemTime::now(),
            };
            
            match FileTransfer::save_file(&msg, "downloads") {
                Ok(path) => {
                    self.messages.push(format!("* File downloaded to: {}", path));
                }
                Err(e) => {
                    self.messages.push(format!("* Error downloading file: {}", e));
                }
            }
        }
        Ok(())
    }

    fn download_all_files(&mut self) -> Result<(), Box<dyn Error>> {
        for i in 0..self.received_files.len() {
            self.download_file(i)?;
        }
        Ok(())
    }

    fn add_message(&mut self, msg: Message) {
        let formatted = match &msg {
            Message::Text { username, content, timestamp } => {
                format!("[{}] {}: {}", self.format_time(*timestamp), username, content)
            }
            Message::File { username, filename, size, timestamp, data } => {
                // Store the file for later viewing/downloading
                self.received_files.push(FileInfo {
                    filename: filename.clone(),
                    size: *size,
                    data: data.clone(),
                    sender: username.clone(),
                });
                format!("[{}] {} shared file: {} ({} bytes) - Press F1 to view files", 
                    self.format_time(*timestamp), username, filename, size)
            }
            Message::UserJoined { username, timestamp } => {
                format!("[{}] * {} joined the chat", self.format_time(*timestamp), username)
            }
            Message::UserLeft { username, timestamp } => {
                format!("[{}] * {} left the chat", self.format_time(*timestamp), username)
            }
            Message::System { content, timestamp } => {
                format!("[{}] * {}", self.format_time(*timestamp), content)
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
        
        match FileTransfer::read_file_with_username(filepath, &self.username) {
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
