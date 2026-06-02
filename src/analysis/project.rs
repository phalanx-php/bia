use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::UNIX_EPOCH;

use super::parse_project_file_payload;
use super::records::{
    CodeNodeRecord, DeclarationRecord, ParseErrorRecord, ReferenceRecord, SourceFileRecord,
    SpanRecord, TokenRecord,
};

const PROJECT_CACHE_LIMIT: usize = 8;

#[derive(Clone, Debug)]
pub(crate) struct ProjectIndexCache {
    entries: Arc<Mutex<HashMap<String, IndexedProject>>>,
    limit: usize,
}

impl ProjectIndexCache {
    pub(crate) fn new() -> Self {
        Self {
            entries: Arc::new(Mutex::new(HashMap::new())),
            limit: PROJECT_CACHE_LIMIT,
        }
    }

    pub(crate) fn index(&self, root: &str) -> Result<IndexedProject, String> {
        let root_path = normalize_root(root)?;
        let root_key = root_path.to_string_lossy().into_owned();
        let scan = scan_project_files(&root_path)?;

        if let Ok(cache) = self.entries.lock()
            && let Some(index) = cache.get(&root_key)
            && index.fingerprint == scan.fingerprint
        {
            return Ok(index.clone());
        }

        let index = build_project_index(root_key.clone(), scan);

        self.store(root_key, index.clone());

        Ok(index)
    }

    fn store(&self, root_key: String, index: IndexedProject) {
        let Ok(mut cache) = self.entries.lock() else {
            return;
        };

        if !cache.contains_key(&root_key) && cache.len() >= self.limit {
            let evicted = cache.keys().min().cloned();

            if let Some(evicted) = evicted {
                cache.remove(&evicted);
            }
        }

        cache.insert(root_key, index);
    }
}

#[derive(Clone, Debug)]
pub(crate) struct IndexedProject {
    pub(crate) root: String,
    fingerprint: Vec<FileFingerprint>,
    pub(crate) files: Vec<SourceFileRecord>,
    pub(crate) declarations: Vec<DeclarationRecord>,
    pub(crate) tokens: Vec<TokenRecord>,
    pub(crate) nodes: Vec<CodeNodeRecord>,
    pub(crate) references: Vec<ReferenceRecord>,
    pub(crate) errors: Vec<ParseErrorRecord>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FileFingerprint {
    path: String,
    len: u64,
    modified_ns: u128,
    content_hash: u64,
}

#[derive(Clone, Debug)]
struct ProjectFile {
    relative: String,
    contents: Vec<u8>,
    fingerprint: FileFingerprint,
}

#[derive(Clone, Debug)]
struct ProjectScan {
    files: Vec<ProjectFile>,
    fingerprint: Vec<FileFingerprint>,
    errors: Vec<ParseErrorRecord>,
}

fn normalize_root(root: &str) -> Result<PathBuf, String> {
    let root = root.trim();
    if root.is_empty() {
        return Err("project root must not be empty".to_string());
    }

    let path = Path::new(root);
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| Path::new(".").to_path_buf())
            .join(path)
    };

    let path = path
        .canonicalize()
        .map_err(|error| format!("failed to resolve project root: {error}"))?;

    if !path.is_dir() {
        return Err("project root must be a directory".to_string());
    }

    Ok(path)
}

fn scan_project_files(root: &Path) -> Result<ProjectScan, String> {
    let mut files = Vec::new();
    let mut errors = Vec::new();
    collect_project_files_into(root, root, &mut files, &mut errors)?;

    files.sort_by(|left, right| left.relative.cmp(&right.relative));

    let fingerprint = files
        .iter()
        .map(|file| file.fingerprint.clone())
        .collect::<Vec<_>>();

    Ok(ProjectScan {
        files,
        fingerprint,
        errors,
    })
}

fn collect_project_files_into(
    root: &Path,
    directory: &Path,
    files: &mut Vec<ProjectFile>,
    errors: &mut Vec<ParseErrorRecord>,
) -> Result<(), String> {
    let entries = fs::read_dir(directory)
        .map_err(|error| format!("failed to read project directory: {error}"))?;

    for entry in entries {
        let entry = entry.map_err(|error| format!("failed to read project entry: {error}"))?;

        let path = entry.path();

        let file_type = entry
            .file_type()
            .map_err(|error| format!("failed to inspect project entry: {error}"))?;

        if file_type.is_dir() {
            if !is_excluded_directory(&entry.file_name().to_string_lossy()) {
                collect_project_files_into(root, &path, files, errors)?;
            }

            continue;
        }

        if !file_type.is_file() || path.extension().and_then(|ext| ext.to_str()) != Some("php") {
            continue;
        }

        let metadata = entry
            .metadata()
            .map_err(|error| format!("failed to inspect PHP source file: {error}"))?;

        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");

        let modified_ns = metadata
            .modified()
            .ok()
            .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |duration| duration.as_nanos());

        let contents = match fs::read(&path) {
            Ok(contents) => contents,
            Err(error) => {
                errors.push(file_error(
                    &relative,
                    format!("failed to read PHP source file: {error}"),
                ));

                continue;
            }
        };

        let content_hash = content_hash(&contents);

        files.push(ProjectFile {
            relative: relative.clone(),
            contents,
            fingerprint: FileFingerprint {
                path: relative,
                len: metadata.len(),
                modified_ns,
                content_hash,
            },
        });
    }

    Ok(())
}

fn is_excluded_directory(name: &str) -> bool {
    matches!(
        name,
        ".git"
            | ".aimind"
            | ".claude"
            | ".daemon8"
            | "build"
            | "dist"
            | "node_modules"
            | "storage"
            | "target"
            | "var"
            | "vendor"
    )
}

fn build_project_index(root: String, scan: ProjectScan) -> IndexedProject {
    let mut indexed = IndexedProject {
        root,
        fingerprint: scan.fingerprint,
        files: Vec::new(),
        declarations: Vec::new(),
        tokens: Vec::new(),
        nodes: Vec::new(),
        references: Vec::new(),
        errors: scan.errors,
    };

    for file in scan.files {
        let payload = parse_project_file_payload(&file.relative, file.contents);

        indexed.files.push(payload.file);

        indexed.errors.extend(
            payload
                .errors
                .into_iter()
                .map(|error| error.with_file(&file.relative)),
        );

        indexed.tokens.extend(
            payload
                .tokens
                .into_iter()
                .map(|token| token.with_file(&file.relative)),
        );

        indexed.declarations.extend(
            payload
                .declarations
                .into_iter()
                .map(|declaration| declaration.with_file(&file.relative)),
        );

        indexed.nodes.extend(
            payload
                .nodes
                .into_iter()
                .map(|node| node.with_file(&file.relative)),
        );

        indexed.references.extend(
            payload
                .references
                .into_iter()
                .map(|reference| reference.with_file(&file.relative)),
        );
    }

    indexed
}

fn content_hash(contents: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    contents.hash(&mut hasher);
    hasher.finish()
}

fn file_error(file: &str, message: String) -> ParseErrorRecord {
    ParseErrorRecord {
        message: format!("{file}: {message}"),
        span: SpanRecord {
            start_offset: 0,
            end_offset: 0,
            start_line: 1,
            start_column: 1,
            end_line: 1,
            end_column: 1,
        },
    }
}
