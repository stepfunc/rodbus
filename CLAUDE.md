# rodbus Codebase Guidelines

## Licensing and AI-Assisted Development

@AGENTS.md

Read `LICENSE.txt` before substantial analysis, copying, extraction,
translation, or reuse. Follow the imported agent policy. Applicable
written license terms and authorizations govern.

## Git Commit Guidelines
- **Never take credit**: Do not add "Generated with Claude Code" or "Co-Authored-By: Claude" to commit messages
- Keep commit messages professional and focused on the changes

## Build, Test, Lint Commands
- Build: `cargo build [--features tls,serial,serialization]`
- `tls` (ring) and `tls-aws-lc` select mutually exclusive crypto providers, so `--all-features` doesn't build. For the AWS-LC build select the package, since other workspace crates enable `tls`: `cargo build -p rodbus --no-default-features --features tls-aws-lc,serial`
- Run the core crate's tests: `cargo test -p rodbus --features ffi,serialization`
- Run all tests: `cargo test`
- Run specific test: `cargo test -p rodbus --lib -- tcp::server::tests::accepts_after_accept_errors`
- Run an example: `cargo run -p example-server -- tcp` or `cargo run -p example-client -- tcp` (also `rtu`, `tls-ca`, `tls-self-signed`)
- Format code: `cargo fmt --all`
- Check lints: `cargo clippy --workspace --all-targets -- -D warnings`; CI also runs clippy on `rodbus` for each feature set in `.github/workflows/ci.yml`
- Logging: the examples log at `INFO`, set in their `main` (`RUST_LOG` has no effect)
- No-panic check: `RUSTFLAGS="--cfg rodbus_no_panic" CARGO_PROFILE_RELEASE_LTO=thin CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 cargo +1.99 test -p rodbus --lib --release --no-run` (the toolchain CI pins; use a separate `CARGO_TARGET_DIR`). A link error names a function marked `no_panic` that the optimizer couldn't prove panic-free; see `rodbus/src/no_panic_check.rs`. Logging and async I/O are outside the proof
- Semver: CI's `semver` job checks every PR against its base; locally, `cargo semver-checks -p rodbus --baseline-rev origin/main --only-explicit-features --features default,ffi,serialization --release-type minor` (with a separate `CARGO_TARGET_DIR`)
- Releases: see `RELEASING.md`

## Dependency Security Advisories
- The `audit` (cargo-audit) and `osv-scanner` CI jobs fail when an advisory is published against a locked dependency, including on unrelated PRs. Check whether `main` is affected, then fix it in its own PR against `main` (usually `cargo update -p <crate>`) and rebase open PRs once it merges
- The PR description must say whether rodbus, as configured, is exposed and why, citing the code that decides it. This analysis is what determines whether released versions need a patch or an announcement
- To assess exposure, check what the released lockfiles (`git show <tag>:Cargo.lock`) pinned and how rodbus uses the affected code. For TLS: `sfio-rustls-config` builds the trust store from `RootCertStore::empty()` plus only the configured CA file (no system or `webpki-roots` certificates), `CertificateMode::AuthorityBased` uses the webpki verifiers against that store, and `CertificateMode::SelfSigned` rejects any intermediates and compares the peer certificate byte for byte without building a chain
- Add a `:shield:` changelog entry naming the advisory and the exposure. Rust users pick up a fixed dependency on their next `cargo update`, so a lockfile update only changes the prebuilt bindings (C/C++, .NET, Java); say so
- If released versions are exposed in a realistic configuration, raise a patch release with the maintainer; otherwise the fix ships in the next release

## Code Style Guidelines
- The workspace lints in the root `Cargo.toml` apply to every crate that inherits them (`[lints] workspace = true`), which is all of them except `rodbus-ffi` and `rodbus-ffi-java`
- **No unsafe code**: Unsafe code is forbidden (`unsafe_code = "forbid"`)
- **Documentation**: All public items must be documented (`missing_docs = "deny"`)
- **API Design**: Avoid unreachable public items (`unreachable_pub = "deny"`)
- **Semver**: Don't break the public Rust API or the generated bindings' APIs
- **Error Handling**: Use proper error types and propagation (no panics in library code)
- **Panic exceptions**: `unwrap`, `expect`, indexing and unchecked arithmetic are allowed in library code only where input can't reach them: the invariant that protects them is local and trivially true, or they can only fire after another panic already happened (e.g. a poisoned mutex). Once the corresponding Clippy lints are denied, each gets a narrow `#[expect(lint, reason = "...")]`; until then an `#[expect]` would go unfulfilled and fail the build
- **Naming**: Follow Rust naming conventions (snake_case for functions/variables, CamelCase for types)
- **Imports**: Organize imports by standard lib, external crates, then internal modules
- **Features**: Support compile-time features (tls, tls-aws-lc, serial, serialization)
- **Minimum Rust Version**: 1.94+ (the workspace `rust-version`, checked by the `msrv` CI job)

## Reference Documents
- The private repository `stepfunc/protocol-reference`, checked out next to this one at `../protocol-reference`, is where to look up what the Modbus specifications require; the Modbus material is under `protocols/modbus/`, and it records our interpretation decisions in `protocols/modbus/decisions/`
- Read its `CLAUDE.md` and `protocols/modbus/CLAUDE.md` first and follow their guidance on reading, citing, and interpreting the specifications
- Its rules for maintaining that repository (such as committing straight to `main`) apply only there, not here
- If it isn't checked out, ask the user for it rather than working from memory of the spec
- **Keep it private**: its contents are not public. Never copy text, tables, or figures from it into code, comments, docs, commit messages, issues, PRs, or attachments here. Cite by document and clause instead (e.g. "MODBUS Application Protocol Specification V1.1b3 6.3")
