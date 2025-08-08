# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Fork Context

This is StuMason's fork of TrailBase implementing fixes for issue #122. See FORK_PLAN.md for the implementation strategy.

## Essential Commands

### Build & Development

```bash
# First time setup (required)
git submodule update --init --recursive  # CRITICAL: Initialize git submodules first!
pnpm install

# Build development binary
cargo build --bin trail

# Build release binary  
cargo build --release --bin trail

# Run TrailBase locally
./target/debug/trail run

# Run with dev mode (permissive CORS for frontend development)
./target/debug/trail run --dev

# Auto-rebuild and restart on file changes (recommended for development)
cargo install cargo-watch  # One-time setup

# Option 1: Watch only Rust source, skip assets rebuild (RECOMMENDED)
# This avoids the infinite rebuild loop caused by frontend changes
cargo watch -w crates/core/src -w crates/cli/src -s "./target/debug/trail run --dev"

# Option 2: If you need to rebuild on Rust changes
cargo watch -w crates/core/src -w crates/cli/src -s "cargo build -p trailbase-cli --bin trail && ./target/debug/trail run --dev"

# Note: The original command below causes infinite rebuild loops due to build.rs watching frontend files
# cargo watch -x "run -p trailbase-cli --bin trail -- run --dev"  # DON'T USE

# Format all code (Rust + JS)
make format

# Run all checks (lint, format, types)
make check
```

### Testing

```bash
# Run all Rust tests
cargo test --workspace

# Run specific test
cargo test --package trailbase-core test_name

# Run admin UI tests
pnpm -C crates/assets/js/admin test

# Full test suite (via pre-commit)
pre-commit run --all-files
```

### Admin UI Development

```bash
# Admin UI is in crates/assets/js/admin/
cd crates/assets/js/admin

# Development server (requires TrailBase running)
# This serves the UI from http://localhost:3000/_/admin with hot reload
pnpm dev

# Build admin UI (ONLY needed for production/embedding into binary)
pnpm build

# IMPORTANT: Only needed when NOT using pnpm dev
# After building the admin UI, rebuild the Rust binary to embed the new UI
cargo build -p trailbase-cli --bin trail

# The built UI gets embedded into the Rust binary
# Note: The build.rs script watches for changes in js/admin/src/components and js/admin/src/lib
# This can cause infinite rebuild loops with cargo watch if not configured properly
```

## Architecture Overview

### Core Components

**HTTP Error Flow** (Critical for issue #122):

1. `trailbase-core/src/admin/error.rs` - AdminError type converts errors to HTTP responses
2. Errors are logged via `log::error!()` macro to stdout only
3. HTTP metadata goes to `_logs` table but error messages are lost
4. Admin UI (`LogsPage.tsx`) only sees HTTP status, not error details

**Database Schema**:

- TrailBase uses internal tables prefixed with `_` (e.g., `_logs`, `_users`)
- Logs table schema is managed by migrations in `trailbase-core/migrations/`
- Access via `trailbase-sqlite` crate which wraps rusqlite

**Admin UI Stack**:

- SolidJS framework (not React!)
- Located in `crates/assets/js/admin/src/`
- Toast notifications use custom implementation in `components/ui/toast.tsx`
- Forms use controlled components with nullable field checkboxes

### Key File Locations

**For Form Fixes (PR 1)**:

- Settings forms: `crates/assets/js/admin/src/components/settings/EmailSettings.tsx`
- Form components: `crates/assets/js/admin/src/components/FormFields.tsx`
- SMTP config specifically in EmailSettings.tsx

**For Toast Improvements (PR 2)**:

- Toast implementation: `crates/assets/js/admin/src/components/ui/toast.tsx`
- Toast usage: Search for `toast.error` or `showToast` calls
- Consider replacing with Sonner or react-hot-toast equivalent for SolidJS

**For Error Logging (PR 3)**:

- Error response: `crates/core/src/admin/error.rs`
- Logging middleware: `crates/core/src/logging.rs`
- Log storage: `crates/core/src/admin/list_logs.rs`
- UI display: `crates/assets/js/admin/src/components/logs/LogsPage.tsx`

## Development Workflow

1. Make changes in appropriate files
2. For Rust changes: `cargo build --bin trail` 
3. For UI changes: `pnpm -C crates/assets/js/admin build`
4. Test locally: `./target/debug/trail run`
5. Access admin UI at: `http://localhost:4000/_admin`

## Testing Changes

For SMTP testing, use mailpit or similar:

```bash
docker run -p 1025:1025 -p 8025:8025 axllent/mailpit
```

## Important Notes

- Admin UI uses **SolidJS**, not React - syntax is similar but not identical
- All UI assets get embedded into the binary - must rebuild Rust after UI changes
- Database migrations auto-run on startup
- Logs are stored in SQLite, accessible via admin UI
- Error messages currently only go to stdout/stderr, never to database (the main issue)

## Protocol Buffer / Protobuf Notes

When modifying `.proto` files:
1. TypeScript bindings: Run `pnpm run proto` in `crates/assets/js/admin/`
2. Rust bindings: Auto-generated during `cargo build`
3. **IMPORTANT**: Rust protobuf generates PascalCase enum variants (e.g., `SmtpEncryption::None`) 
   while proto files use SCREAMING_SNAKE_CASE (e.g., `SMTP_ENCRYPTION_NONE`)
4. Use `i32` type for optional enums in Rust, then convert with `try_from()`

## Frontend Development

When running the admin UI dev server (`pnpm dev` in `crates/assets/js/admin/`):
- Frontend runs on `http://localhost:3000/_/admin`
- Backend must be running on port 4000 (use `--dev` flag for CORS)
- API calls are made to `http://localhost:4000` in dev mode