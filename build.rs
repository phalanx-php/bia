use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=embedded/bootstrap.php");
    println!("cargo:rerun-if-changed=embedded/dory-runtime.tar");
    println!("cargo:rerun-if-env-changed=DORY_STATIC_PHP_PREFIX");

    let prefix = env::var_os("DORY_STATIC_PHP_PREFIX")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let home = env::var("HOME")
                .expect("HOME not set. Set DORY_STATIC_PHP_PREFIX to your PHP build root.");

            PathBuf::from(format!("{home}/.ripht/php"))
        });

    let lib_dir = prefix.join("lib");

    if !lib_dir.is_dir() {
        println!(
            "cargo:warning=Dory static PHP libraries not found at {}; run scripts/build-static-engine.sh or set DORY_STATIC_PHP_PREFIX",
            prefix.display()
        );

        return;
    }

    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib=static=php");

    let mut libs = fs::read_dir(&lib_dir)
        .expect("failed to read Dory static PHP lib directory")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .collect::<Vec<_>>();

    libs.sort();

    for path in libs {
        if path.extension().is_some_and(|ext| ext == "a") {
            if let Some(file_name) = path.file_stem().and_then(|name| name.to_str()) {
                if file_name.starts_with("lib") && file_name != "libphp" {
                    println!("cargo:rustc-link-lib=static={}", &file_name[3..]);
                }
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-lib=framework=CoreServices");
        println!("cargo:rustc-link-lib=framework=CoreFoundation");
        println!("cargo:rustc-link-lib=framework=SystemConfiguration");
        println!("cargo:rustc-link-lib=framework=Security");
    }

    #[cfg(unix)]
    println!("cargo:rustc-link-lib=resolv");
}
