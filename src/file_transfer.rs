use crate::message::Message;
use std::error::Error;
use std::fs;
use std::path::Path;

#[allow(dead_code)]
pub struct FileTransfer;

impl FileTransfer {
    #[allow(dead_code)]
    pub fn read_file(filepath: &str) -> Result<Message, Box<dyn Error>> {
        let path = Path::new(filepath);
        
        if !path.exists() {
            return Err(format!("File not found: {}", filepath).into());
        }

        let filename = path.file_name()
            .ok_or("Invalid filename")?
            .to_string_lossy()
            .to_string();

        let data = fs::read(path)?;
        
        // For now, we'll use a placeholder username - this should come from the caller
        Ok(Message::new_file("user".to_string(), filename, data))
    }

    #[allow(dead_code)]
    pub fn save_file(msg: &Message, download_dir: &str) -> Result<String, Box<dyn Error>> {
        if let Message::File { filename, data, .. } = msg {
            let download_path = Path::new(download_dir);
            
            // Create download directory if it doesn't exist
            fs::create_dir_all(download_path)?;
            
            let file_path = download_path.join(filename);
            fs::write(&file_path, data)?;
            
            Ok(file_path.to_string_lossy().to_string())
        } else {
            Err("Message is not a file".into())
        }
    }

    #[allow(dead_code)]
    pub fn get_file_info(filepath: &str) -> Result<(String, u64), Box<dyn Error>> {
        let path = Path::new(filepath);
        
        if !path.exists() {
            return Err(format!("File not found: {}", filepath).into());
        }

        let filename = path.file_name()
            .ok_or("Invalid filename")?
            .to_string_lossy()
            .to_string();

        let metadata = fs::metadata(path)?;
        let size = metadata.len();
        
        Ok((filename, size))
    }
}
