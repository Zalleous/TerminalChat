use crate::message::Message;
use std::error::Error;
use std::fs;
use std::path::Path;

pub struct FileTransfer;

impl FileTransfer {
    /// Sanitize filename to prevent path traversal attacks
    /// Removes any path components and returns just the safe filename
    fn sanitize_filename(filename: &str) -> Result<String, Box<dyn Error>> {
        let path = Path::new(filename);

        // Extract just the file name component, rejecting any path traversal
        let safe_name = path.file_name()
            .ok_or("Invalid filename: no file name component")?
            .to_string_lossy()
            .to_string();

        // Additional check: ensure the filename doesn't contain path separators
        if safe_name.contains('/') || safe_name.contains('\\') {
            return Err("Invalid filename: contains path separators".into());
        }

        // Reject hidden files (starting with .) for security
        if safe_name.starts_with('.') {
            return Err("Invalid filename: hidden files not allowed".into());
        }

        // Ensure it's not empty
        if safe_name.is_empty() {
            return Err("Invalid filename: empty".into());
        }

        Ok(safe_name)
    }

    pub fn read_file_with_username(filepath: &str, username: &str) -> Result<Message, Box<dyn Error>> {
        let path = Path::new(filepath);
        
        if !path.exists() {
            return Err(format!("File not found: {}", filepath).into());
        }

        let filename = path.file_name()
            .ok_or("Invalid filename")?
            .to_string_lossy()
            .to_string();

        let data = fs::read(path)?;
        
        Ok(Message::new_file(username.to_string(), filename, data))
    }

    pub fn save_file(msg: &Message, download_dir: &str) -> Result<String, Box<dyn Error>> {
        if let Message::File { filename, data, .. } = msg {
            // Sanitize filename to prevent path traversal attacks
            let safe_filename = Self::sanitize_filename(filename)?;

            let download_path = Path::new(download_dir);

            // Create download directory if it doesn't exist
            fs::create_dir_all(download_path)?;

            // Use sanitized filename
            let file_path = download_path.join(&safe_filename);

            // Verify the final path is still within download_dir (double check)
            let canonical_download = download_path.canonicalize()?;
            let canonical_file = if file_path.exists() {
                file_path.canonicalize()?
            } else {
                // For new files, check the parent directory
                file_path.parent()
                    .ok_or("Invalid file path")?
                    .canonicalize()?
                    .join(&safe_filename)
            };

            if !canonical_file.starts_with(&canonical_download) {
                return Err("Security: Path traversal detected".into());
            }

            fs::write(&file_path, data)?;

            Ok(file_path.to_string_lossy().to_string())
        } else {
            Err("Message is not a file".into())
        }
    }

}
