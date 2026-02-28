# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Initial release of remotebuild
- SSH-based remote build proxy functionality
- Git-aware file synchronization for faster incremental builds
- Multiple output levels (minimal, normal, verbose)
- Configurable build commands and artifact patterns
- SSH connection pooling with control sockets
- Custom spinner for minimal output mode

### Security
- Proper shell command escaping to prevent injection
- SSH key-based authentication support

## [0.1.0] - 2025-02-28

### Added
- Initial public release
- Support for YAML configuration files
- Remote file synchronization via rsync
- Build command execution on remote servers
- Artifact retrieval from remote builds
- Cross-platform support (Linux, macOS, Windows with WSL)
