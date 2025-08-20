#!/bin/bash
set -e

echo "🔧 Running local CI checks for proc-daemon..."

echo "📋 Checking formatting..."
cargo fmt -- --check

echo "🔍 Running Clippy..."
cargo clippy --all-targets --all-features -- -D warnings

echo "🧪 Running tests..."
cargo test --all-features

echo "📚 Checking documentation..."
cargo doc --all-features --no-deps

echo "🏗️ Building all feature combinations..."
cargo build --no-default-features
cargo build --no-default-features --features tokio
cargo build --no-default-features --features async-std
cargo build --features full

echo "⚡ Running benchmarks (dry run)..."
cargo bench --no-run

echo "✅ All CI checks passed!"
