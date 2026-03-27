/// Characters that could enable command injection via CreateProcessW or cmd.exe.
const SHELL_METACHARACTERS: &[char] = &[
    '&', '|', ';', '>', '<', '$', '`', '(', ')', '{', '}', '!', '^', '"', '\'',
];

/// Validate extra arguments for the spawn command.
///
/// Every token must start with `-` (a CLI flag) and contain no shell metacharacters.
/// Values must use `=` syntax (e.g. `--model=sonnet`), not space-separated.
pub fn validate_args(args: &str) -> Result<String, String> {
    let trimmed = args.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }

    let tokens: Vec<&str> = trimmed.split_whitespace().collect();
    for token in &tokens {
        if let Some(bad) = token.chars().find(|c| SHELL_METACHARACTERS.contains(c)) {
            return Err(format!(
                "Invalid argument '{token}': contains forbidden character '{bad}'"
            ));
        }
        if !token.starts_with('-') {
            return Err(format!(
                "Invalid argument '{token}': must be a flag starting with '-'"
            ));
        }
    }

    Ok(tokens.join(" "))
}

/// Validate a working directory path.
///
/// Must be a local, absolute, existing directory. UNC paths are rejected
/// to prevent implicit NTLM credential leaks via SMB authentication.
pub fn validate_cwd(cwd: &str) -> Result<String, String> {
    let cwd = cwd.trim();

    // Reject UNC paths before any other checks (\\server\share is "absolute" per std::path)
    if cwd.starts_with("\\\\") {
        return Err(
            "Working directory must be a local path, not a network path (UNC)".to_string(),
        );
    }

    let path = std::path::Path::new(cwd);

    if !path.is_absolute() {
        return Err("Working directory must be an absolute path".to_string());
    }

    if !path.is_dir() {
        return Err(format!(
            "Working directory does not exist or is not a directory: '{cwd}'"
        ));
    }

    // Canonicalize and re-check: std::fs::canonicalize on Windows produces \\?\ prefix.
    // \\?\C:\... is safe (extended local), \\?\UNC\... is unsafe (extended UNC).
    if let Ok(canonical) = std::fs::canonicalize(path) {
        let canonical_str = canonical.to_string_lossy();
        if canonical_str.starts_with("\\\\?\\UNC\\") {
            return Err(
                "Working directory must be a local path, not a network path (UNC)".to_string(),
            );
        }
    }

    Ok(cwd.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- validate_args ---

    #[test]
    fn args_empty() {
        assert_eq!(validate_args(""), Ok(String::new()));
    }

    #[test]
    fn args_whitespace_only() {
        assert_eq!(validate_args("   "), Ok(String::new()));
    }

    #[test]
    fn args_single_flag() {
        assert_eq!(
            validate_args("--dangerously-skip-permissions"),
            Ok("--dangerously-skip-permissions".to_string())
        );
    }

    #[test]
    fn args_flag_with_equals_value() {
        assert_eq!(
            validate_args("--model=sonnet"),
            Ok("--model=sonnet".to_string())
        );
    }

    #[test]
    fn args_multiple_flags() {
        assert_eq!(
            validate_args("--model=sonnet --verbose"),
            Ok("--model=sonnet --verbose".to_string())
        );
    }

    #[test]
    fn args_short_flag() {
        assert_eq!(validate_args("-v"), Ok("-v".to_string()));
    }

    #[test]
    fn args_rejects_ampersand() {
        assert!(validate_args("& whoami").is_err());
    }

    #[test]
    fn args_rejects_semicolon() {
        assert!(validate_args("--flag; rm").is_err());
    }

    #[test]
    fn args_rejects_pipe() {
        assert!(validate_args("--flag | evil").is_err());
    }

    #[test]
    fn args_rejects_dollar() {
        assert!(validate_args("$(evil)").is_err());
    }

    #[test]
    fn args_rejects_non_flag() {
        assert!(validate_args("notaflag").is_err());
    }

    #[test]
    fn args_rejects_mixed_flag_and_non_flag() {
        assert!(validate_args("--ok notaflag").is_err());
    }

    #[test]
    fn args_rejects_backtick() {
        assert!(validate_args("--flag `evil`").is_err());
    }

    #[test]
    fn args_rejects_redirect_in_token() {
        assert!(validate_args("--flag>out.txt").is_err());
    }

    #[test]
    fn args_normalizes_whitespace() {
        assert_eq!(
            validate_args("  --flag1   --flag2  "),
            Ok("--flag1 --flag2".to_string())
        );
    }

    // --- validate_cwd ---

    #[test]
    fn cwd_rejects_unc_path() {
        assert!(validate_cwd("\\\\server\\share").is_err());
    }

    #[test]
    fn cwd_rejects_relative_path() {
        assert!(validate_cwd("relative/path").is_err());
    }

    #[test]
    fn cwd_rejects_nonexistent_dir() {
        assert!(validate_cwd("C:\\nonexistent_dir_xyz_quell_test").is_err());
    }

    #[test]
    fn cwd_accepts_userprofile() {
        let home = std::env::var("USERPROFILE").expect("USERPROFILE must be set");
        assert!(validate_cwd(&home).is_ok());
    }

    #[test]
    fn cwd_accepts_windows_dir() {
        assert!(validate_cwd("C:\\Windows").is_ok());
    }
}
