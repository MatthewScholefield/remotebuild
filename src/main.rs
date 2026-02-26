use anyhow::{anyhow, Context, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};
use shell_escape::escape;
use std::borrow::Cow;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::env;
use std::fmt;

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

    /// Output level: minimal, normal, or verbose (default: minimal)
    #[serde(default)]
    output: String,
}

impl Config {
    fn output_level(&self) -> OutputLevel {
        match self.output.to_lowercase().as_str() {
            "verbose" | "v" => OutputLevel::Verbose,
            "normal" | "n" => OutputLevel::Normal,
            _ => OutputLevel::Minimal,
        }
    }
}

#[derive(Clone, Copy)]
enum OutputLevel {
    Minimal,  // Single \r-overwriting lines
    Normal,   // Multi-line status with clear start/end
    Verbose,  // All details
}

fn default_remote_path() -> String {
    "~/remotebuild-cache".to_string()
}

fn default_true() -> bool {
    true
}

/// Get the SSH control socket path for connection sharing
fn ssh_control_path(host: &str) -> String {
    // Use XDG cache directory or fallback to temp
    let cache_dir = dirs::cache_dir().unwrap_or_else(|| env::temp_dir());
    let control_dir = cache_dir.join("remotebuild");
    let _ = fs::create_dir_all(&control_dir);

    // Sanitize hostname for use in filename
    let safe_host = host.replace(|c: char| !c.is_alphanumeric() && c != '-' && c != '.', "_");
    control_dir.join(format!("control_{}", safe_host)).to_string_lossy().to_string()
}

/// Ensure SSH control master connection is established
fn ensure_ssh_connection(config: &Config) -> Result<()> {
    let control_path = ssh_control_path(&config.host);

    // Check if control socket already exists and is valid
    if Path::new(&control_path).exists() {
        // Test connection with a simple command
        let test_result = Command::new("ssh")
            .arg("-o")
            .arg("ControlMaster=no")
            .arg("-o")
            .arg(format!("ControlPath={}", control_path))
            .arg("-o")
            .arg("ConnectTimeout=2")
            .arg(&config.host)
            .arg("true")
            .output();

        if let Ok(output) = test_result {
            if output.status.success() {
                // Connection is still alive
                return Ok(());
            }
        }
    }

    // Start new control master connection in background
    Command::new("ssh")
        .arg("-N")
        .arg("-M")
        .arg("-o")
        .arg("ControlMaster=auto")
        .arg("-o")
        .arg("ControlPersist=10m")
        .arg("-o")
        .arg(format!("ControlPath={}", control_path))
        .arg(&config.host)
        .spawn()
        .context("Failed to start SSH control master")?;

    // Give it a moment to establish
    std::thread::sleep(std::time::Duration::from_millis(100));

    Ok(())
}

/// Helper to add SSH control options to a command
fn add_ssh_control_args(cmd: &mut Command, config: &Config) {
    let control_path = ssh_control_path(&config.host);
    cmd.arg("-o")
        .arg(format!("ControlPath={}", control_path));
}

/// Create a Command with SSH control path pre-configured
fn ssh_command(config: &Config) -> Command {
    let mut cmd = Command::new("ssh");
    add_ssh_control_args(&mut cmd, config);
    cmd.arg(&config.host);
    cmd
}

/// Get the SSH control path as a string (for rsync -e flag)
fn ssh_control_path_arg(config: &Config) -> String {
    format!("ssh -o ControlPath={}", ssh_control_path(&config.host))
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

    /// Output level (minimal, normal, verbose). Overrides config file
    #[arg(short, long)]
    output: Option<String>,
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
    let mut config: Config = load_config(&config_path)?;

    // Override output level if specified on CLI
    if let Some(output) = args.output {
        config.output = output;
    }

    // Run the remote build
    run_remote_build(&project_dir, &config, args.force_full_sync)?;

    Ok(())
}

fn load_config(path: &Path) -> Result<Config> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read config file: {}", path.display()))?;

    serde_yaml::from_str(&content)
        .map_err(|e| anyhow!("Failed to parse config file: {} - {}", path.display(), e))
}

fn run_remote_build(project_dir: &Path, config: &Config, force_full_sync: bool) -> Result<()> {
    let output = config.output_level();

    match output {
        OutputLevel::Minimal => {
            print!("🚀 Remote Build: {} ", config.host);
            use std::io::Write;
            std::io::stdout().flush().ok();
        }
        OutputLevel::Normal | OutputLevel::Verbose => {
            println!("🚀 Remote Build Proxy");
            println!("   Host: {}", config.host);
            println!("   Project: {}", project_dir.display());
            println!();
        }
    }

    // Step 1: Sync files to remote
    sync_to_remote(project_dir, config, force_full_sync)?;

    // Step 2: Run build command on remote and stream output
    run_remote_build_command(config)?;

    // Step 3: Copy artifacts back
    sync_artifacts(config)?;

    match output {
        OutputLevel::Minimal => {
            println!("\r✅ Build complete!");
        }
        OutputLevel::Normal | OutputLevel::Verbose => {
            println!();
            println!("✅ Build complete!");
        }
    }

    Ok(())
}

/// Print a status message that can be overwritten with \r
fn print_status(level: OutputLevel, message: &str) {
    match level {
        OutputLevel::Minimal => {
            print!("\r{} ", message);
            use std::io::Write;
            std::io::stdout().flush().ok();
        }
        OutputLevel::Normal => {
            println!("{}", message);
        }
        OutputLevel::Verbose => {
            println!("{}", message);
        }
    }
}

/// Clear the current status line (for minimal mode)
fn clear_status(level: OutputLevel) {
    if matches!(level, OutputLevel::Minimal) {
        print!("\r{: <80}\r", ' ');
        use std::io::Write;
        std::io::stdout().flush().ok();
    }
}

fn sync_to_remote(project_dir: &Path, config: &Config, force_full_sync: bool) -> Result<()> {
    let output = config.output_level();

    print_status(output, "📦 Syncing files...");

    // Ensure SSH connection is established for reuse
    ensure_ssh_connection(config)?;

    // Use remote_path as-is (it should be the full destination path)
    let remote_full_path = &config.remote_path;

    // Create remote directory if it doesn't exist
    let mkdir_cmd = format!("mkdir -p {}", escape(Cow::Borrowed(remote_full_path.as_str())));
    run_ssh_command(config, &mkdir_cmd)?;

    // Build rsync command
    let mut rsync_cmd = Command::new("rsync");
    rsync_cmd.arg("-avz");

    match output {
        OutputLevel::Verbose => rsync_cmd.arg("-v"),
        _ => rsync_cmd.arg("--quiet"),
    };

    // Add delete flag to keep remote in sync
    rsync_cmd.arg("--delete");

    // Add SSH control path for connection reuse
    rsync_cmd.arg("-e")
        .arg(ssh_control_path_arg(config));

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

    // In minimal mode, the status line stays, no additional output needed
    if matches!(output, OutputLevel::Normal) {
        println!("   ✓ Sync complete");
        println!();
    }

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

fn run_remote_build_command(config: &Config) -> Result<()> {
    let output = config.output_level();

    clear_status(output);
    print_status(output, "🔨 Building...");

    // Don't escape the cd path, just the build command if needed
    let cmd = format!("cd {} && {}", config.remote_path, config.build_command);

    // Run SSH command with output streaming
    let status = if matches!(output, OutputLevel::Verbose) {
        ssh_command(config).arg(&cmd).status()?
    } else {
        // Clear the status line before showing build output
        clear_status(output);
        ssh_command(config)
            .arg(&cmd)
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .status()?
    };

    if !status.success() {
        return Err(anyhow!("Remote build command failed with exit code: {:?}", status));
    }

    if matches!(output, OutputLevel::Normal) {
        println!();
        println!("   ✓ Build complete");
        println!();
    }

    Ok(())
}

fn sync_artifacts(config: &Config) -> Result<()> {
    let output = config.output_level();

    print_status(output, "📥 Copying artifacts...");

    for artifact in &config.artifacts {
        let mut rsync_cmd = Command::new("rsync");
        rsync_cmd.arg("-avz");

        match output {
            OutputLevel::Verbose => rsync_cmd.arg("-v"),
            _ => rsync_cmd.arg("--quiet"),
        };

        // Use SSH control path for connection reuse
        rsync_cmd.arg("-e")
            .arg(ssh_control_path_arg(config));

        // Copy from remote to current directory
        rsync_cmd.arg(format!("{}:{}/{}", config.host, config.remote_path, artifact));
        rsync_cmd.arg("."); // Copy to current directory

        let status = rsync_cmd.status()
            .context("Failed to run rsync for artifacts")?;

        if !status.success() {
            // Non-fatal: just warn about missing artifacts
            eprintln!("   ⚠ Warning: Could not copy artifact: {}", artifact);
        } else {
            if matches!(output, OutputLevel::Verbose) {
                println!("   ✓ Copied: {}", artifact);
            }
        }
    }

    if matches!(output, OutputLevel::Normal) {
        println!("   ✓ Artifacts downloaded");
        println!();
    }

    Ok(())
}

fn run_ssh_command(config: &Config, cmd: &str) -> Result<()> {
    let output = ssh_command(config)
        .arg(cmd)
        .output()
        .context("Failed to run SSH command")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("SSH command failed: {}", stderr));
    }

    Ok(())
}
