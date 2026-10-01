# Changelog

## [0.1.0] - 2026-10-02

Initial release.

- Allocation-free, safe Rust CIL decoding with an iced-style streaming API.
- All 219 standard instructions and semantic prefixes, 43 Mono operation
  identities across five dialect selections, and seven reserved descriptors.
- Borrowed instruction bytes and switch tables, typed operands, branch targets,
  opcode metadata, prefix grouping, and optional structural prefix validation.
- Explicit registration of private opcode maps and fixed/count-prefixed payloads.
- Formatting with symbol resolution, caller-provided buffers, and optional alloc/std.
- Rust 2024 edition, Rust 1.85 minimum, and no third-party runtime dependencies.
- Independent fixtures for .NET Framework 2/4 and .NET 5/8/9/10: 3,056 methods,
  66,130 instructions, and 40 switches. Historical Mono layouts and deterministic
  structured fuzz regressions supplement the standard-opcode coverage.

[0.1.0]: https://github.com/valeria-org/fNET/releases/tag/v0.1.0
