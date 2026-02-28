# Contributing to remotebuild

Thank you for your interest in contributing to remotebuild! This document provides guidelines for contributing to the project.

## Development Setup

1. Clone the repository:
   ```bash
   git clone https://github.com/yourusername/remotebuild.git
   cd remotebuild
   ```

2. Install Rust (if not already installed):
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

3. Build the project:
   ```bash
   cargo build --release
   ```

## Code Style

This project uses standard Rust tooling for code quality:

- **Formatting**: Use `cargo fmt` to format code
- **Linting**: Use `cargo clippy` to check for issues
- **Testing**: Use `cargo test` to run tests

Before submitting a pull request, please run:
```bash
cargo fmt
cargo clippy -- -D warnings
cargo test
```

## Making Changes

1. Fork the repository
2. Create a new branch for your feature or bugfix
3. Make your changes following the code style guidelines
4. Write tests for new functionality
5. Ensure all tests pass
6. Commit your changes with a clear commit message
7. Push to your fork and submit a pull request

## Commit Messages

Please write clear, descriptive commit messages. For example:

- `Add: Support for custom SSH port configuration`
- `Fix: Properly handle spaces in file paths`
- `Docs: Update README with new examples`

## Testing

While the project currently has limited test coverage, we encourage adding tests for new features. Tests should be placed in the `tests/` directory or as inline tests in the source code.

## Reporting Issues

When reporting bugs or suggesting features, please include:

- Your operating system and version
- The version of remotebuild you're using
- Steps to reproduce the issue (for bugs)
- Expected vs actual behavior
- Any relevant configuration or logs

## License

By contributing to remotebuild, you agree that your contributions will be licensed under the MIT License.
