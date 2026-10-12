//! Build tooling for AssetCraft.
//!
//! - `cargo xtask ci`: fmt check, clippy (warnings are errors), tests, layers. Stops at the first failure.
//! - `cargo xtask layers`: check the crate layering table against `cargo metadata`.

mod layers;

use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    let task = std::env::args().nth(1);
    let result = match task.as_deref() {
        Some("ci") => ci(),
        Some("layers") => check_layers(),
        _ => {
            eprintln!("usage: cargo xtask <ci|layers>");
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
    check_layers()?;
    println!("xtask ci: all checks passed");
    Ok(())
}

fn run(step: &str, args: &[&str]) -> Result<(), String> {
    println!("xtask ci: {step}");
    let status =
        Command::new(cargo()).args(args).status().map_err(|e| format!("{step}: could not start cargo: {e}"))?;
    if status.success() { Ok(()) } else { Err(format!("{step} failed")) }
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
