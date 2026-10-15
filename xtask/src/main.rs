//! Build tooling for AssetCraft.
//!
//! - `cargo xtask ci`: fmt check, clippy (warnings are errors), tests, the same two with the opt-in
//!   `heic` feature (thumbs, engine, ui-egui), layers, deny. Stops at the first failure.
//! - `cargo xtask layers`: check the crate layering table against `cargo metadata`.
//! - `cargo xtask deny`: licence and source checks (`deny.toml`) with cargo-deny, if installed;
//!   without it the step is skipped with a message, not failed.

mod layers;

use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    let task = std::env::args().nth(1);
    let result = match task.as_deref() {
        Some("ci") => ci(),
        Some("layers") => check_layers(),
        Some("deny") => deny(),
        _ => {
            eprintln!("usage: cargo xtask <ci|layers|deny>");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("xtask: {message}");
            ExitCode::FAILURE
        }
    }
}

fn ci() -> Result<(), String> {
    run("fmt", &["fmt", "--all", "--check"])?;
    run("clippy", &["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"])?;
    run("test", &["test", "--workspace", "--quiet"])?;
    // The HEIC decoder is an opt-in feature (HEVC patents); build, lint and test it too so the
    // module doesn't rot. Feature syntax `crate/feature` enables it under every listed crate.
    let heic = [
        "-p",
        "assetcraft-thumbs",
        "-p",
        "assetcraft-engine",
        "-p",
        "assetcraft-ui-egui",
        "--features",
        "assetcraft-thumbs/heic",
    ];
    let mut clippy_heic = vec!["clippy"];
    clippy_heic.extend(heic);
    clippy_heic.extend(["--all-targets", "--", "-D", "warnings"]);
    run("clippy (heic)", &clippy_heic)?;
    let mut test_heic = vec!["test", "--quiet"];
    test_heic.extend(heic);
    run("test (heic)", &test_heic)?;
    check_layers()?;
    deny()?;
    println!("xtask ci: all checks passed");
    Ok(())
}

fn run(step: &str, args: &[&str]) -> Result<(), String> {
    println!("xtask ci: {step}");
    let status =
        Command::new(cargo()).args(args).status().map_err(|e| format!("{step}: could not start cargo: {e}"))?;
    if status.success() { Ok(()) } else { Err(format!("{step} failed")) }
}

/// `cargo deny check licenses sources` when cargo-deny is installed; a clear skip otherwise.
/// (Advisories aren't gated here: they change daily; run `cargo deny check advisories` by hand.)
fn deny() -> Result<(), String> {
    println!("xtask: deny");
    let installed = Command::new(cargo())
        .args(["deny", "--version"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    if !installed {
        println!("xtask: deny skipped: cargo-deny is not installed (`cargo install cargo-deny --locked` to enable)");
        return Ok(());
    }
    let status = Command::new(cargo())
        .args(["deny", "check", "licenses", "sources"])
        .status()
        .map_err(|e| format!("deny: could not start cargo deny: {e}"))?;
    if status.success() { Ok(()) } else { Err("deny: licence or source check failed (see deny.toml)".into()) }
}

fn check_layers() -> Result<(), String> {
    println!("xtask: layers");
    let output = Command::new(cargo())
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .output()
        .map_err(|e| format!("layers: could not start cargo metadata: {e}"))?;
    if !output.status.success() {
        return Err(format!("layers: cargo metadata failed: {}", String::from_utf8_lossy(&output.stderr)));
    }
    let meta: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|e| format!("layers: bad cargo metadata JSON: {e}"))?;
    let crates = layers::from_metadata(&meta)?;
    let errors = layers::check(&crates);
    if errors.is_empty() {
        println!("xtask: layers ok ({} crates)", crates.len());
        Ok(())
    } else {
        for e in &errors {
            eprintln!("  {e}");
        }
        Err(format!("layers: {} violation(s)", errors.len()))
    }
}

fn cargo() -> String {
    std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string())
}
