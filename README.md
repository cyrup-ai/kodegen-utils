# kodegen_utils

[![License](https://img.shields.io/badge/license-Apache%202.0%20OR%20MIT-blue.svg)](LICENSE.md)
[![Rust](https://img.shields.io/badge/rust-nightly-orange.svg)](https://www.rust-lang.org)

Memory-efficient, blazing-fast utilities for code generation agents. Part of the [KODEGEN.ᴀɪ](https://kodegen.ai) ecosystem.

## Overview

`kodegen_utils` provides high-performance text processing utilities designed specifically for AI coding assistants and MCP (Model Context Protocol) tools. It focuses on solving the invisible character problems that plague AI-generated code edits through sophisticated character-level analysis and fuzzy string matching.

## Features

- **🔍 Fuzzy String Matching**: Levenshtein distance-based search with recursive optimization
- **🔬 Character-Level Analysis**: Deep diagnostics for invisible Unicode issues (zero-width chars, mixed line endings, tabs vs spaces)
- **📊 Visual Diffs**: Character-precise diff visualization in format: `prefix{-removed-}{+added+}suffix`
- **⚡ Async Telemetry**: Non-blocking logging with fire-and-forget patterns
- **🎯 Smart Suggestions**: Actionable error messages for edit failures
- **💾 LRU Caching**: Optimized performance for repeated analysis operations
- **📈 Usage Tracking**: Built-in statistics for MCP tool operations

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
kodegen_utils = "0.1.0"
```

This crate requires Rust nightly:

```bash
rustup install nightly
rustup default nightly
```

## Usage

### Fuzzy String Matching

Find approximate matches in text using Levenshtein distance:

```rust
use kodegen_utils::fuzzy_search::{
    recursive_fuzzy_index_of_with_defaults,
    get_similarity_ratio,
    levenshtein_distance,
};

let text = "The quick brown fox jumps over the lazy dog";
let result = recursive_fuzzy_index_of_with_defaults(text, "qwick");

println!("Match: {} at position {}-{}", result.value, result.start, result.end);
println!("Distance: {}", result.distance);

// Calculate similarity ratio (0.0 to 1.0)
let similarity = get_similarity_ratio("hello", "hallo");
println!("Similarity: {:.1}%", similarity * 100.0);
```

### Character-Level Diff

Generate visual diffs to identify invisible character differences:

```rust
use kodegen_utils::char_diff::CharDiff;

let expected = "function getUserData()";
let actual = "function  getUserData()"; // Extra space

let diff = CharDiff::new(expected, actual);
println!("{}", diff.format());
// Output: function {--}{+ +}getUserData()

if diff.is_whitespace_only() {
    println!("Difference is only whitespace");
}
```

### Character Analysis

Deep analysis for diagnosing invisible character issues:

```rust
use kodegen_utils::char_analysis::{
    CharCodeData,
    WhitespaceIssue,
    EncodingIssue,
};

// Analysis is automatically cached in LRU cache
let analysis: CharCodeData = analyze_string_diff("expected", "actual");

println!("Report: {}", analysis.report);
println!("Unique chars: {}", analysis.unique_count);

// Check for specific issues
if analysis.has_zero_width {
    println!("Warning: Contains zero-width Unicode characters");
}

for issue in &analysis.whitespace_issues {
    match issue {
        WhitespaceIssue::TabsVsSpaces => println!("Mixed tabs and spaces detected"),
        WhitespaceIssue::MixedLineEndings => println!("Inconsistent line endings"),
        _ => {}
    }
}
```

### Async Edit Logging

Non-blocking telemetry for edit operations:

```rust
use kodegen_utils::edit_log::{get_edit_logger, EditBlockLogEntry, EditBlockResult};
use chrono::Utc;

let logger = get_edit_logger();

let entry = EditBlockLogEntry {
    timestamp: Utc::now(),
    search_text: "old_text".to_string(),
    found_text: Some("old_text".to_string()),
    similarity: Some(1.0),
    execution_time_ms: 15.3,
    exact_match_count: 1,
    expected_replacements: 1,
    fuzzy_threshold: 0.8,
    below_threshold: false,
    diff: None,
    search_length: 8,
    found_length: Some(8),
    file_extension: "rs".to_string(),
    character_codes: None,
    unique_character_count: None,
    diff_length: None,
    result: EditBlockResult::ExactMatch,
};

// Fire-and-forget logging (never blocks)
logger.log(entry);

println!("Logs written to: {}", logger.log_path().display());
```

### User-Facing Suggestions

Generate actionable error messages:

```rust
use kodegen_utils::suggestions::{
    EditFailureReason,
    SuggestionContext,
    Suggestion,
};

let context = SuggestionContext {
    file_path: "src/main.rs".to_string(),
    search_string: "function foo()".to_string(),
    line_number: Some(42),
    log_path: None,
    execution_time_ms: Some(12.5),
};

let reason = EditFailureReason::FuzzyMatchBelowThreshold {
    similarity: 0.65,
    threshold: 0.8,
    found_text: "function bar()".to_string(),
};

let suggestion = Suggestion::for_failure(&reason, &context);
println!("{}\n{}", suggestion.message, suggestion.format());
```

## Architecture

The library is organized into focused modules:

- **`fuzzy_search`**: Levenshtein distance and recursive fuzzy matching
- **`char_diff`**: Character-level diff generation
- **`char_analysis`**: Deep character diagnostics with LRU caching
- **`edit_log`**: Async telemetry for edit operations
- **`fuzzy_logger`**: Async fuzzy search logging
- **`usage_tracker`**: MCP tool usage statistics
- **`suggestions`**: User-facing error messages
- **`line_endings`**: Cross-platform line ending handling

### Performance Design

All logging operations use **fire-and-forget async patterns**:
- Unbounded channels prevent blocking
- Background tasks batch disk writes
- Periodic flushes (5-second intervals)
- Graceful shutdown handling

## Development

### Building

```bash
# Build library
cargo build

# Build with optimizations
cargo build --release
```

### Testing

```bash
# Run all tests
cargo test

# Run specific test
cargo test --test test_fuzzy_search

# Show test output
cargo test -- --nocapture
```

### Linting and Formatting

```bash
# Format code
cargo fmt

# Run clippy
cargo clippy

# Check without building
cargo check
```

## Requirements

- **Rust**: Nightly toolchain (2024 edition)
- **Targets**: `x86_64-apple-darwin`, `wasm32-unknown-unknown`
- **Components**: `rustfmt`, `clippy`

See `rust-toolchain.toml` for exact configuration.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE.md) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE.md) or http://opensource.org/licenses/MIT)

at your option.

## Contributing

Contributions are welcome! This library is part of the KODEGEN.ᴀɪ project.

See the [repository](https://github.com/cyrup-ai/kodegen) for contribution guidelines.

## Links

- **Homepage**: [https://kodegen.ai](https://kodegen.ai)
- **Repository**: [https://github.com/cyrup-ai/kodegen](https://github.com/cyrup-ai/kodegen)
- **Documentation**: [docs.rs](https://docs.rs/kodegen_utils)

---

Built with ❤️ by the KODEGEN.ᴀɪ team
