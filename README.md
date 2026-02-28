# remotebuild

A simple, fast command-line tool for proxying builds to a remote server via SSH. Perfect for development environments where you can't run builds locally (e.g., Termux on Android) but have access to a remote build server.

## Features

- **Fast sync**: Uses rsync with git-aware change detection to only transfer modified files
- **Live output**: Streams build output from the remote server in real-time
- **Zero server setup**: No server-side installation required - just SSH and rsync
- **Simple configuration**: Single YAML file per project
- **Artifact retrieval**: Automatically copies build artifacts back to your local machine

## Requirements

On your local machine:
- SSH client
- rsync
- Git (optional, for git-aware syncing)

On the remote server:
- SSH server
- Build tools for your project
- No installation of remotebuild required!

## Installation

### Pre-built binaries (Recommended)

Download pre-built binaries from the [Releases](https://github.com/MatthewScholefield/remotebuild/releases) page. Available for:
- Linux (x86_64 and ARM64)
- macOS (Intel and Apple Silicon)
- Windows (x86_64)

### From source

```bash
cargo build --release
```

The binary will be at `target/release/remotebuild`.

### Installing

```bash
# Copy to your bin directory
cp target/release/remotebuild ~/bin/

# Or install system-wide
sudo cp target/release/remotebuild /usr/local/bin/
```

## Configuration

Create a `.remotebuild.yaml` file in your project directory:

```yaml
# SSH host to connect to
host: user@hostname  # or just hostname if using SSH config

# Full path on remote server where project will be synced
remote_path: ~/path/to/project

# Build command to run on remote server
build_command: make  # or ./build.sh, cargo build, etc.

# Artifacts to copy back (relative to project root)
artifacts:
  - build/output.bin
  - build/output.elf

# Optional: Additional patterns to exclude from sync
exclude_patterns:
  - "*.log"
  - "temp/"

# Optional: Enable git-aware file syncing (default: true)
git_aware: true

# Optional: Output level - minimal, normal, or verbose (default: minimal)
# - minimal: Single-line status indicators (cleanest output)
# - normal: Multi-line status with completion messages
# - verbose: Detailed file transfer logs
output: minimal
```

## Usage

From your project directory:

```bash
# Basic usage (minimal output)
remotebuild

# Normal output mode
remotebuild -o normal

# Verbose output (shows file transfer details)
remotebuild -o verbose

# Force full sync (ignore git change detection)
remotebuild --force-full-sync

# Specify custom config file
remotebuild -c custom-config.yaml

# Build from different directory
remotebuild -p /path/to/project
```

## How It Works

1. **Sync**: Uses rsync to transfer your project files to the remote server
   - If `git_aware: true`, only syncs files tracked by git (plus untracked files not in .gitignore)
   - Automatically excludes build artifacts, .git, and common build directories

2. **Build**: Runs your build command on the remote server via SSH
   - Streams output in real-time to your local terminal
   - Exit codes are properly propagated

3. **Retrieve**: Uses rsync to copy specified artifacts back to your local machine

## Example: Nintendo DS Development

For Nintendo DS development on Termux (where the toolchain can't run locally):

```yaml
host: my-build-server
remote_path: ~/Code/ds-project
build_command: bash build.sh
artifacts:
  - game.nds
  - game.elf
exclude_patterns:
  - "*.nds"
  - "build/"
```

Then just run `remotebuild` from your project directory on Termux!

## Tips

### SSH Configuration

For the best experience, configure SSH in `~/.ssh/config`:

```
Host my-build-server
    HostName example.com
    User myuser
    # Optional: Use SSH keys for passwordless auth
    IdentityFile ~/.ssh/id_ed25519
```

Then you can use just the host alias in your config:
```yaml
host: my-build-server
```

### Persistent Connections

For faster repeated builds, enable SSH connection sharing in `~/.ssh/config`:

```
Host *
    ControlMaster auto
    ControlPath ~/.ssh/cm/%r@%h:%p
    ControlPersist 10m
```

### Build Speed

- Use `git_aware: true` for incremental builds (only syncs changed files)
- Add generated files to `.gitignore` so they aren't synced unnecessarily
- Use `--force-full-sync` only when you need to resync everything

## License

MIT

## Contributing

Contributions welcome! Please feel free to submit pull requests.
