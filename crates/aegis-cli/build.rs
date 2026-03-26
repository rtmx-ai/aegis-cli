//! Build script: embed git SHA and target triple into the binary.

fn main() {
    // Git SHA
    let sha = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=AEGIS_GIT_SHA={sha}");

    // Target triple
    if let Ok(target) = std::env::var("TARGET") {
        println!("cargo:rustc-env=AEGIS_TARGET={target}");
    }

    // Rebuild if git HEAD changes
    println!("cargo:rerun-if-changed=../../.git/HEAD");
}
