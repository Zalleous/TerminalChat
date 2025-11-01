use std::error::Error;

/// Maximum username length
const MAX_USERNAME_LENGTH: usize = 32;

/// Minimum username length
const MIN_USERNAME_LENGTH: usize = 1;

/// Maximum message length (10 MB)
const MAX_MESSAGE_LENGTH: usize = 10 * 1024 * 1024;

/// Validate username
pub fn validate_username(username: &str) -> Result<(), Box<dyn Error>> {
    let trimmed = username.trim();

    // Check length
    if trimmed.len() < MIN_USERNAME_LENGTH {
        return Err("Username is too short (minimum 1 character)".into());
    }

    if trimmed.len() > MAX_USERNAME_LENGTH {
        return Err(format!("Username is too long (maximum {} characters)", MAX_USERNAME_LENGTH).into());
    }

    // Check for invalid characters
    if !trimmed.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == ' ') {
        return Err("Username contains invalid characters (only alphanumeric, _, -, and spaces allowed)".into());
    }

    // Ensure it's not just whitespace
    if trimmed.is_empty() {
        return Err("Username cannot be empty or only whitespace".into());
    }

    Ok(())
}

/// Validate message content
pub fn validate_message(message: &str) -> Result<(), Box<dyn Error>> {
    // Check length
    if message.len() > MAX_MESSAGE_LENGTH {
        return Err(format!("Message is too long (maximum {} bytes)", MAX_MESSAGE_LENGTH).into());
    }

    // Messages can be empty (valid case)
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_usernames() {
        assert!(validate_username("alice").is_ok());
        assert!(validate_username("bob123").is_ok());
        assert!(validate_username("user_name").is_ok());
        assert!(validate_username("user-name").is_ok());
        assert!(validate_username("Alice Bob").is_ok());
    }

    #[test]
    fn test_invalid_usernames() {
        assert!(validate_username("").is_err());
        assert!(validate_username("   ").is_err());
        assert!(validate_username("a".repeat(33).as_str()).is_err());
        assert!(validate_username("user@name").is_err());
        assert!(validate_username("user#name").is_err());
    }

    #[test]
    fn test_valid_messages() {
        assert!(validate_message("Hello world").is_ok());
        assert!(validate_message("").is_ok());
        assert!(validate_message("Message with special chars !@#$%").is_ok());
    }

    #[test]
    fn test_invalid_messages() {
        let huge_message = "x".repeat(11 * 1024 * 1024);
        assert!(validate_message(&huge_message).is_err());
    }
}
