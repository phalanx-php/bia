use std::path::Path;

use crate::error::BiaError;

pub fn read_exit_code(path: &Path) -> Result<u8, BiaError> {
    let raw = std::fs::read_to_string(path).map_err(|error| {
        BiaError::from_error(
            format!("failed to read BIA_EXIT_FILE {}", path.display()),
            error,
        )
    })?;

    let trimmed = raw.trim();

    if trimmed.is_empty() {
        return Err(BiaError::new(format!(
            "BIA_EXIT_FILE {} was empty",
            path.display()
        )));
    }

    let parsed = trimmed.parse::<u16>().map_err(|error| {
        BiaError::from_error(
            format!(
                "BIA_EXIT_FILE {} contained an invalid exit code",
                path.display()
            ),
            error,
        )
    })?;

    if parsed > u8::MAX as u16 {
        return Err(BiaError::new(format!(
            "BIA_EXIT_FILE {} contained an out-of-range exit code: {parsed}",
            path.display()
        )));
    }

    Ok(parsed as u8)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use tempfile::NamedTempFile;

    use super::*;

    #[test]
    fn reads_trimmed_exit_code() {
        let mut file = NamedTempFile::new().expect("temp file");
        file.write_all(b" 42\n").expect("write");

        assert_eq!(read_exit_code(file.path()).expect("exit code"), 42);
    }

    #[test]
    fn missing_exit_file_fails_closed() {
        let path = std::env::temp_dir().join("bia-missing-exit-code-for-test");
        let error = read_exit_code(&path).expect_err("missing file must fail");

        assert!(error.to_string().contains("failed to read BIA_EXIT_FILE"));
    }

    #[test]
    fn empty_exit_file_fails_closed() {
        let file = NamedTempFile::new().expect("temp file");
        let error = read_exit_code(file.path()).expect_err("empty file must fail");

        assert!(error.to_string().contains("was empty"));
    }

    #[test]
    fn invalid_exit_file_fails_closed() {
        let mut file = NamedTempFile::new().expect("temp file");
        file.write_all(b"nope").expect("write");

        let error = read_exit_code(file.path()).expect_err("invalid file must fail");

        assert!(error.to_string().contains("invalid exit code"));
    }

    #[test]
    fn out_of_range_exit_file_fails_closed() {
        let mut file = NamedTempFile::new().expect("temp file");
        file.write_all(b"256").expect("write");

        let error = read_exit_code(file.path()).expect_err("large code must fail");

        assert!(error.to_string().contains("out-of-range exit code"));
    }
}
