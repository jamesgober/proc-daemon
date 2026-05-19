<div align="center">
    <img width="108xpx" src="../media/proc-rs-orange.svg" alt="High-Performance Process EcoSystem for Rust">
    <h1>
        <strong>Process Daemon</strong>
        <sup>
            <br>
            <sub>DOCUMENTATION</sub>
            <br>
        </sup>
    </h1>
</div>

**Version:** 1.1.1

## Index

### Reference

- **[API.md](./API.md)** — Complete API reference, organized by use case with multiple examples per module. Always current to the latest release.
- **[PERFORMANCE.md](./PERFORMANCE.md)** — Benchmark methodology, headline numbers, and version-over-version regression / improvement notes.
- **[PRINCIPLES.md](./PRINCIPLES.md)** — Code style, contribution norms, and architectural rationale.

### Release notes

Per-version release notes (format modeled on the
[error-forge](https://github.com/jamesgober/error-forge) release docs):

- **[v1.1.1](./release-notes/v1.1.1.md)** *(2026-05-19, docs.rs build fix)*
- **[v1.1.0](./release-notes/v1.1.0.md)** *(2026-05-19, performance + ergonomics)*
- **[v1.0.1](./release-notes/v1.0.1.md)** *(2026-05-18, maintenance + audit patch)*

The aggregate [CHANGELOG.md](../CHANGELOG.md) in the repo root covers
the full release history including pre-1.0 lines.

### v2.0.0 planning

Tracked in [`.dev/V2-ROADMAP.md`](../.dev/V2-ROADMAP.md) (local-only,
gitignored). The v2 line targets:

- `async fn` in trait for `Subsystem::run` (drops `Pin<Box<dyn Future>>` boilerplate).
- Coordinated major dependency bumps (`thiserror 1→2`, `metrics 0.17→0.24`, `notify 6→8`, `toml 0.8→1`, `nix 0.29→0.31`, `windows 0.58→0.62`).
- Removal of the discontinued `async-std` runtime backend.

<br>

<!--
:: COPYRIGHT
============================================================================ -->
<div align="center">
  <br>
  <h2></h2>
  <sup>COPYRIGHT <small>&copy;</small> 2026 <strong>JAMES GOBER.</strong></sup>
</div>
