use anyhow::{anyhow, Context, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};
use shell_escape::escape;
use std::borrow::Cow;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::env;

/// Remote build configuration file
#[derive(Debug, Serialize, Deserialize)]
struct Config {
    /// SSH host to connect to (e.g., "user@host" or just "host")
    host: String,

    /// Remote path where the project will be synced and built
    #[serde(default = "default_remote_path")]
    remote_path: String,

    /// Build command to run on the remote server
    build_command: String,

    /// List of artifact patterns to copy back (relative to project root)
    artifacts: Vec<String>,

    /// Files/directories to exclude from sync (gitignore-style patterns)
    #[serde(default)]
    exclude_patterns: Vec<String>,

    /// Whether to use git to detect changed files for faster sync
    #[serde(default = "default_true")]
    git_aware: bool,

    /// Verbose output
    #[serde(default)]
    verbose: bool,
}

fn default_remote_path() -> String {
    "~/remotebuild-cache".to_string()
}

fn default_true() -> bool {
    true
}

/// Command line arguments
#[derive(Parser, Debug)]
#[command(name = "remotebuild")]
#[command(about = "Proxy builds to a remote server via SSH", long_about = None)]
struct Args {
    /// Path to project directory (defaults to current directory)
    #[arg(short, long)]
    path: Option<PathBuf>,

    /// Config file name (defaults to .remotebuild.yaml)
    #[arg(short, long, default_value = ".remotebuild.yaml")]
    config: String,

    /// Force full sync (ignore git change detection)
    #[arg(long)]
    force_full_sync: bool,

    /// Verbose output
    #[arg(short, long)]
    verbose: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Determine project directory
    let project_dir = if let Some(path) = args.path {
        fs::canonicalize(path)?
    } else {
        env::current_dir()?
    };

    if !project_dir.is_dir() {
        return Err(anyhow!("Project path is not a directory: {}", project_dir.display()));
    }

    // Load config
    let config_path = project_dir.join(&args.config);
    let config: Config = load_config(&config_path)?;

    // Merge verbose flag from CLI
    let verbose = args.verbose || config.verbose;

    // Run the remote build
    run_remote_build(&project_dir, &config, verbose, args.force_full_sync)?;

    Ok(())
}

fn load_config(path: &Path) -> Result<Config> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read config file: {}", path.display()))?;

    serde_yaml::from_str(&content)
        .map_err(|e| anyhow!("Failed to parse config file: {} - {}", path.display(), e))
}

fn run_remote_build(project_dir: &Path, config: &Config, verbose: bool, force_full_sync: bool) -> Result<()> {
    println!("🚀 Remote Build Proxy");
    println!("   Host: {}", config.host);
    println!("   Project: {}", project_dir.display());
    println!();

    // Step 1: Sync files to remote
    sync_to_remote(project_dir, config, verbose, force_full_sync)?;

    // Step 2: Run build command on remote and stream output
    run_remote_build_command(config, verbose)?;

    // Step 3: Copy artifacts back
    sync_artifacts(config, verbose)?;

    println!();
    println!("✅ Build complete!");

    Ok(())
}

fn sync_to_remote(project_dir: &Path, config: &Config, verbose: bool, force_full_sync: bool) -> Result<()> {
    println!("📦 Syncing files to remote...");

    // Use remote_path as-is (it should be the full destination path)
    let remote_full_path = &config.remote_path;

    // Create remote directory if it doesn't exist
    let mkdir_cmd = format!("mkdir -p {}", escape(Cow::Borrowed(remote_full_path.as_str())));
    run_ssh_command(config, &mkdir_cmd, false)?;

    // Build rsync command
    let mut rsync_cmd = Command::new("rsync");
    rsync_cmd.arg("-avz");

    if verbose {
        rsync_cmd.arg("-v");
    } else {
        rsync_cmd.arg("--quiet");
    }

    // Add delete flag to keep remote in sync
    rsync_cmd.arg("--delete");

    // Add exclusions
    rsync_cmd.arg("--exclude=.git");
    rsync_cmd.arg("--exclude=.gitignore");
    rsync_cmd.arg("--exclude=*.nds");
    rsync_cmd.arg("--exclude=*.elf");
    rsync_cmd.arg("--exclude=build/");
    rsync_cmd.arg("--exclude=.ninja_*");
    rsync_cmd.arg("--exclude=compile_commands.json");

    for pattern in &config.exclude_patterns {
        rsync_cmd.arg(format!("--exclude={}", pattern));
    }

    // If git-aware and not forcing full sync, only sync tracked and new files
    let temp_file = if config.git_aware && !force_full_sync {
        if let Ok(tracked_files) = get_git_files(project_dir) {
            if !tracked_files.is_empty() {
                // Use --files-from to sync only tracked files
                // We need to write the list to a temp file
                let temp_dir = dirs::cache_dir().unwrap_or_else(|| env::temp_dir());
                let temp_file = temp_dir.join(format!("remotebuild_{}", std::process::id()));

                fs::write(&temp_file, tracked_files.join("\n"))?;

                rsync_cmd.arg(format!("--files-from={}", temp_file.display()));

                Some(temp_file)
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    // Add source and destination
    rsync_cmd.arg(format!("{}/", project_dir.display()));
    rsync_cmd.arg(format!("{}:{}/", config.host, remote_full_path));

    // Run rsync
    let status = rsync_cmd.status()
        .context("Failed to run rsync. Make sure rsync is installed.")?;

    // Clean up temp file if we created one
    if let Some(temp_file) = temp_file {
        let _ = fs::remove_file(&temp_file);
    }

    if !status.success() {
        return Err(anyhow!("rsync failed with exit code: {:?}", status));
    }

    println!("   ✓ Sync complete");
    println!();

    Ok(())
}

fn get_git_files(project_dir: &Path) -> Result<Vec<String>> {
    let output = Command::new("git")
        .args(["ls-files"])
        .current_dir(project_dir)
        .output()
        .context("Failed to run git ls-files. Is this a git repository?")?;

    if !output.status.success() {
        return Ok(vec![]);
    }

    let tracked = String::from_utf8_lossy(&output.stdout);
    let mut files: Vec<String> = tracked.lines().map(|s| s.to_string()).collect();

    // Also get untracked files that aren't ignored
    let output_untracked = Command::new("git")
        .args(["ls-files", "--others", "--exclude-standard"])
        .current_dir(project_dir)
        .output()?;

    if output_untracked.status.success() {
        let untracked = String::from_utf8_lossy(&output_untracked.stdout);
        for file in untracked.lines() {
            if !file.is_empty() {
                files.push(file.to_string());
            }
        }
    }

    Ok(files)
}

fn run_remote_build_command(config: &Config, verbose: bool) -> Result<()> {
    println!("🔨 Running build on remote...");

    // Don't escape the cd path, just the build command if needed
    let cmd = format!("cd {} && {}", config.remote_path, config.build_command);

    // Run SSH command with output streaming
    let status = if verbose {
        Command::new("ssh")
            .arg(&config.host)
            .arg(&cmd)
            .status()?
    } else {
        Command::new("ssh")
            .arg(&config.host)
            .arg(&cmd)
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .status()?
    };

    if !status.success() {
        return Err(anyhow!("Remote build command failed with exit code: {:?}", status));
    }

    println!();
    println!("   ✓ Build complete");
    println!();

    Ok(())
}

fn sync_artifacts(config: &Config, verbose: bool) -> Result<()> {
    println!("📥 Copying artifacts back...");

    for artifact in &config.artifacts {
        let mut rsync_cmd = Command::new("rsync");
        rsync_cmd.arg("-avz");

        if verbose {
            rsync_cmd.arg("-v");
        } else {
            rsync_cmd.arg("--quiet");
        }

        // Copy from remote to current directory
        rsync_cmd.arg(format!("{}:{}/{}", config.host, config.remote_path, artifact));
        rsync_cmd.arg("."); // Copy to current directory

        let status = rsync_cmd.status()
            .context("Failed to run rsync for artifacts")?;

        if !status.success() {
            // Non-fatal: just warn about missing artifacts
            eprintln!("   ⚠ Warning: Could not copy artifact: {}", artifact);
        } else {
            if verbose {
                println!("   ✓ Copied: {}", artifact);
            }
        }
    }

    println!("   ✓ Artifacts downloaded");
    println!();

    Ok(())
}

fn run_ssh_command(config: &Config, cmd: &str, _verbose: bool) -> Result<()> {
    let output = Command::new("ssh")
        .arg(&config.host)
        .arg(cmd)
        .output()
        .context("Failed to run SSH command")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("SSH command failed: {}", stderr));
    }

    Ok(())
}
