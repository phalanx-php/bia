use std::io::{Cursor, Write};
use std::path::PathBuf;
use tempfile::{NamedTempFile, TempDir};

const TAR_BYTES: &[u8] = include_bytes!("../embedded/bia-runtime.tar");
const BOOTSTRAP_BYTES: &[u8] = include_bytes!("../embedded/bootstrap.php");

pub struct EmbeddedRuntime {
    pub bootstrap: NamedTempFile,
    pub runtime_dir: TempDir,
}

impl EmbeddedRuntime {
    pub fn extract() -> Result<Self, Box<dyn std::error::Error>> {
        let runtime_dir = TempDir::new()?;

        let cursor = Cursor::new(TAR_BYTES);
        let mut archive = tar::Archive::new(cursor);
        archive.unpack(runtime_dir.path())?;

        let mut bootstrap = NamedTempFile::with_suffix(".php")?;

        bootstrap.write_all(BOOTSTRAP_BYTES)?;

        bootstrap.flush()?;

        Ok(Self {
            bootstrap,
            runtime_dir,
        })
    }

    pub fn runtime_path(&self) -> PathBuf {
        self.runtime_dir.path().to_path_buf()
    }
}
