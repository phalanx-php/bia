use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=embedded/bootstrap.php");
    println!("cargo:rerun-if-changed=embedded/bia-runtime.tar");
    println!("cargo:rerun-if-changed=scripts/build-static-engine.sh");
    println!("cargo:rerun-if-env-changed=BIA_STATIC_PHP_PREFIX");

    let prefix = env::var_os("BIA_STATIC_PHP_PREFIX")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let manifest_dir = env::var_os("CARGO_MANIFEST_DIR").expect(
                "CARGO_MANIFEST_DIR not set. Set BIA_STATIC_PHP_PREFIX to your PHP build root.",
            );

            PathBuf::from(manifest_dir).join(".ripht/php")
        });

    let lib_dir = prefix.join("lib");

    let link_flags = lib_dir.join("bia-link-flags.txt");

    println!("cargo:rerun-if-changed={}", prefix.display());
    println!("cargo:rerun-if-changed={}", lib_dir.display());
    println!("cargo:rerun-if-changed={}", link_flags.display());

    if !lib_dir.is_dir() {
        println!(
            "cargo:warning=BIA_STATIC_RUNTIME_MISSING: static PHP libraries not found at {}; run ./scripts/build-static-engine.sh or set BIA_STATIC_PHP_PREFIX",
            prefix.display()
        );

        return;
    }

    if link_flags.is_file() {
        emit_link_flags(&link_flags);

        return;
    }

    if lib_dir.join("libphp.a").is_file() {
        panic!(
            "BIA_LINKER_MANIFEST_REQUIRED: Bia linker manifest not found at {}. Run ./scripts/build-static-engine.sh to regenerate the static PHP runtime.",
            link_flags.display()
        );
    }

    println!(
        "cargo:warning=BIA_STATIC_RUNTIME_MISSING: static PHP libphp.a not found at {}; run ./scripts/build-static-engine.sh or set BIA_STATIC_PHP_PREFIX",
        lib_dir.display()
    );
}

fn emit_link_flags(path: &Path) {
    let manifest = fs::read_to_string(path).expect("failed to read Bia linker manifest");

    for line in manifest.lines() {
        let Some((_, value)) = line.split_once('=') else {
            continue;
        };

        emit_tokens(&split_shell_words(value));
    }
}

fn emit_tokens(tokens: &[String]) {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    if target_os == "linux" {
        emit_linux_tokens(tokens);

        return;
    }

    emit_cargo_tokens(tokens, &target_os);
}

fn emit_linux_tokens(tokens: &[String]) {
    for token in tokens {
        if token.starts_with("-L") && token.len() > 2 {
            println!("cargo:rustc-link-search=native={}", &token[2..]);
        }
    }

    println!("cargo:rustc-link-arg=-Wl,--start-group");

    for token in tokens {
        if token.starts_with("-l") && token.len() > 2 {
            println!("cargo:rustc-link-arg={token}");
        }
    }

    println!("cargo:rustc-link-arg=-Wl,--end-group");

    for token in tokens {
        match token.as_str() {
            "-pthread" => println!("cargo:rustc-link-arg=-pthread"),
            token if token.starts_with("-Wl,") => println!("cargo:rustc-link-arg={token}"),
            _ => {}
        }
    }
}

fn emit_cargo_tokens(tokens: &[String], target_os: &str) {
    let mut i = 0;

    while i < tokens.len() {
        match tokens[i].as_str() {
            "-framework" => {
                if let Some(name) = tokens.get(i + 1) {
                    println!("cargo:rustc-link-lib=framework={name}");
                    i += 2;
                    continue;
                }
            }
            "-pthread" => {
                println!("cargo:rustc-link-arg=-pthread");
            }
            token if token.starts_with("-L") && token.len() > 2 => {
                println!("cargo:rustc-link-search=native={}", &token[2..]);
            }
            token if token.starts_with("-l") && token.len() > 2 => {
                let lib = &token[2..];

                if target_os == "macos" && lib == "stdc++" {
                    i += 1;
                    continue;
                }

                println!("cargo:rustc-link-lib={lib}");
            }
            token => {
                println!("cargo:rustc-link-arg={token}");
            }
        }

        i += 1;
    }
}

fn split_shell_words(value: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quote = None;

    for ch in value.chars() {
        match (quote, ch) {
            (Some(active), ch) if ch == active => {
                quote = None;
            }
            (Some(_), ch) => current.push(ch),
            (None, '\'' | '"') => {
                quote = Some(ch);
            }
            (None, ch) if ch.is_whitespace() => {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
            }
            (None, ch) => current.push(ch),
        }
    }

    if !current.is_empty() {
        words.push(current);
    }

    words
}
