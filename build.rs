fn main() {
    // Derive version from `git describe` (e.g. "v0.4.0" or "v0.4.0-3-gabcdef").
    // Strip the leading 'v' to match Cargo version convention.
    // Fall back to the Cargo.toml version when git is unavailable.
    let version = git_describe()
        .unwrap_or_else(|| std::env::var("CARGO_PKG_VERSION").unwrap_or_default());

    println!("cargo:rustc-env=GIT_VERSION={}", version);

    // Rebuild whenever the HEAD ref or any tag changes.
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/packed-refs");
    println!("cargo:rerun-if-changed=.git/refs/tags");
}

fn git_describe() -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["describe", "--tags", "--dirty=-modified"])
        .output()
        .ok()?;

    if output.status.success() {
        let s = String::from_utf8(output.stdout).ok()?;
        Some(s.trim().trim_start_matches('v').to_string())
    } else {
        None
    }
}
