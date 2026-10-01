# fNET

[![crates.io](https://img.shields.io/crates/v/fNET.svg)](https://crates.io/crates/fNET)
[![Documentation](https://docs.rs/fNET/badge.svg)](https://docs.rs/fNET)
[![MIT license](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A modern Rust disassembler for raw .NET Common Intermediate Language (CIL),
with an API shaped like fARM64 and iced-x86.

The library has **no dependencies**, forbids unsafe code, and defaults to
**`no_std` without `alloc`**. `Decoder`, `Instruction`, opcode inspection,
prefix grouping, switch target iteration, and formatting work without a heap.
Rust 1.85 or newer is required (edition 2024).

```rust
use fNET::{BufSink, Code, Decoder, DecoderOptions, FmtFormatter, Formatter};

let bytes = [0x1f, 42, 0x2b, 0xfc, 0x2a];
let mut decoder = Decoder::new(&bytes, 0x1000, DecoderOptions::NONE);
let constant = decoder.decode();
assert_eq!(constant.code(), Code::LdcI4S);
assert_eq!(constant.immediate_i32(), Some(42));

let branch = decoder.decode();
assert_eq!(branch.near_branch_target(), Some(0x1000));
let mut buffer = [0; 64];
let mut output = BufSink::new(&mut buffer);
FmtFormatter::new().format(&branch, &mut output).unwrap();
assert_eq!(output.as_str(), "br.s IL_1000");
```

Install the crate with:

```toml
[dependencies]
fNET = "0.1"
```

## Coverage

- **219 standard CIL instructions and semantic prefixes**, covering every
  assigned one-byte and `0xFE` encoding. All six prefixes are supported:
  `constrained.`, `no.`, `readonly.`, `tail.`, `unaligned.`, and `volatile.`.
  `no.` is included explicitly because CoreCLR's opcode table reserves its slot
  and Reflection.Emit does not expose it.
- **43 distinct Mono operation identities**, covering the 35 modern `0xF0`
  instructions, retired `mono_jit_attach` / `mono_jit_detach`, the historical
  `mono_get_last_error`, and five early conversion/free helpers. Five explicit
  dialect selections cover the conflicting maps found across all 46 revisions
  of Mono's main-branch opcode-table history. This includes the current
  `mono_remap_ovf_exc` (`F0 23`) and `mono_ldvirtftn_delegate` (`F0 24`).
  See [the source audit](research/MONO-AUDIT.md) for exact revisions and corrections.
- **Private extension registration** for unused/reserved slots, including
  fixed byte payloads, counted byte payloads, and all standard operand layouts.
- **Seven reserved marker descriptors** (`F8`–`FD`, `FF`) available through
  `decode_reserved`. `FE` always introduces a two-byte instruction; it cannot
  be decoded as a standalone `prefix1`.
- All operand layouts: signed integers, unsigned local/argument indices,
  raw IEEE float payloads, metadata tokens, signed branches, and switches.
- Standard aliases such as `brnull`, `brzero`, `brinst`, `endfault`,
  `ldelem.any`, `stelem.any`, and `ldind.u8`.
- Per-opcode operand layout, exact encoding, canonical mnemonic, opcode kind,
  static stack behaviour, and control-flow classification.

The catalogue contains 269 operation identities: 219 standard, 43 Mono, and
seven reserved. Modern/retired decoding uses 263 descriptors; the three early
maps contain 7, 12, and 15 descriptors, and the legacy profile replaces F01B's
saver with the getter. Shared operations retain their `Code` across maps.
`Code::Invalid` and `Code::Custom` are separate sentinels. Sources are pinned in
[NOTICE](NOTICE); catalogue generation is reproducible offline.

This library accepts a method's **code bytes**. PE headers, metadata tables,
exception clauses, ReadyToRun code, Mono JIT IR and interpreter wordcode use
separate formats. Unregistered experimental or future assignments are reported
as invalid. Metadata tokens and Mono wrapper-data indices are retained
without requiring an assembly reader. Static stack effects for calls and returns
can depend on signatures; `count()` returns `None` in those cases.

## Decoder API

`Decoder::new(bytes, ip, options)` follows fARM64's constructor shape; CIL has
no bitness argument. `with_ip` is an alias. The decoder supports `decode`,
`decode_into`, iced-style `decode_out`, `can_decode`, `last_error`, `position`,
`set_position`, `ip`, `set_ip`, and iteration by value or mutable reference.

```rust
use fNET::{Decoder, DecoderOptions, Instruction};

let bytes = [0x00, 0x2a];
let mut decoder = Decoder::new(&bytes, 0, DecoderOptions::ECMA335);
let mut instruction = Instruction::default();
while decoder.can_decode() {
    decoder.decode_out(&mut instruction);
    assert!(!instruction.is_invalid());
}
```

`DecoderOptions::NONE` / `default()` enable all implemented extensions and reject
reserved markers, following fARM64's default extension policy.
`DecoderOptions::ECMA335` disables Mono. `FeatureSet::MONO` enables the selected
Mono map; `MONO_LEGACY` additionally enables retired identities. Reserved bytes:

```rust
use fNET::{DecoderOptions, FeatureSet};
let options = DecoderOptions {
    features: FeatureSet::ALL,
    decode_reserved: true,
};
```

Instructions are `Copy` values borrowing the original input. Keep that input
alive when storing instructions. `bytes()` returns the original complete span;
`offset()` is the byte-slice position and `ip()` is the caller's address mapping.
`Code` names exact operations; `Mnemonic` and `Code::canonical()` normalize short
and macro forms. `op_count()` counts explicit operands, while helpers such as
`variable_index()` and `immediate_i32()` also expose implicit macro values.
Typed helpers return `Option`, avoiding ambiguous zero values.

All CIL fields are read in little-endian byte order without alignment assumptions.
Branch targets use the address after the instruction. Switch displacements use
the address after the entire switch table. Addresses wrap modulo `2^64`.
Instruction lengths use `usize`; switches have no fixed case-count limit and
retain a borrowed displacement table instead of allocating one target per case.
Mono's pointer-named `InlineI` operands are **four-byte IDs / wrapper-data
indices**, not host-sized native pointers.

### Malformed input

Every decode on nonempty input advances:

- An unassigned opcode consumes its opcode byte or two-byte escape sequence.
- A truncated opcode, operand, or switch consumes the remaining tail once.
- A recognized disabled extension consumes the full encoded instruction.
- A reserved marker consumes one byte.
- A direct decode at EOF returns an empty invalid instruction with
  `NoMoreBytes`; iteration simply ends.

Failures carry `Code::Invalid` and a `DecodeError` on both the instruction and
`Decoder::last_error()`. Raw consumed bytes remain available for diagnostics.
Overflowing switch counts are checked against the input before accessing the
table. Seeking beyond the slice returns `InvalidPosition` without changing state.

### Prefix grouping

Default decoding reports each semantic prefix as an instruction. Use
`decode_prefixed()` or `into_prefixed()` to group a prefix sequence with its target:

```rust
use fNET::{Code, Decoder, DecoderOptions};
let bytes = [0xfe, 0x13, 0xfe, 0x12, 1, 0x4a];
let grouped = Decoder::new(&bytes, 0, DecoderOptions::NONE).decode_prefixed();
assert_eq!(grouped.instruction.code(), Code::LdindI4);
assert_eq!(grouped.prefixes().count(), 2);
assert_eq!(grouped.len(), 6);
```

Order, duplicates, and raw prefix operand values are preserved. A trailing sequence is
reported as `MissingInstruction`. `PrefixedInstruction` exposes the complete span
and its first IP; its inner instruction keeps its own opcode IP and offset.
Offsets from `prefixes()` are relative to `prefix_bytes()`.

`grouped.validate_prefixes(PrefixValidationOptions::Ecma335)` checks all six
prefixes: alignment values, nonzero `no.` masks, per-bit target compatibility,
duplicates, constrained type-token tags, and `tail.` calls followed by `ret`.
The `Runtime` policy also allows `constrained.` on `call` and `ldftn`, per
[.NET's ECMA augments](https://github.com/dotnet/runtime/blob/main/docs/design/specs/Ecma-335-Augments.md).
`validate_prefixes_with_resolver` accepts a `PrefixMetadataResolver` to validate
`readonly.` calls to an array's special Address method; unresolved calls return
`MetadataRequired`. These structural checks do not verify stack types, token
existence, method signatures, exception regions, or branch entry boundaries.

### Historical and private dialects

Select `MonoDialect::July2002`, `Early2002`, `May2004`, `Legacy`, or `Modern`
with the builder. Selection is explicit because the same F0 bytes mean different
operations. Early maps include additions made before their next renumbering;
`Legacy` uses the post-2004 map with the pre-July-2019 F01B getter. It retains the
current map's additive opcodes and retired attach/detach descriptors; it is a
disassembly superset, rather than a particular runtime's executable instruction set.

```rust
use fNET::{Code, Decoder, DecoderOptions, MonoDialect};
let bytes = [0xf0, 1, 255];
let instruction = Decoder::new(&bytes, 0, DecoderOptions::NONE)
    .with_mono_dialect(MonoDialect::May2004).decode();
assert_eq!(instruction.code(), Code::MonoFunc2);
assert_eq!(instruction.immediate_u8(), Some(255));
assert_eq!(instruction.encoding(), 0xf001);
```

Use `Instruction::encoding()` and `op_code()` for the selected dialect's exact
encoding and metadata. `Code::value()` is an operation ID (early-only and getter
IDs use E100..E105); `Code::op_code()` gives the default descriptor, and
`Code::from_encoding()` resolves the modern/retired map only.

```rust
use fNET::{Decoder, DecoderOptions, ExtensionTable, OpCodeInfo, OperandKind, Operand};
let descriptors = [OpCodeInfo::custom(0xf780, 2, "vendor.blob", OperandKind::ShortInlineData)];
let extensions = ExtensionTable::new(&descriptors).unwrap();
let bytes = [0xf7, 0x80, 2, 0xaa, 0xbb];
let instruction = Decoder::with_extensions(&bytes, 0, DecoderOptions::NONE, extensions).decode();
assert_eq!(instruction.name(), "vendor.blob");
assert_eq!(instruction.operand(), Operand::Bytes(&[0xaa, 0xbb]));
```

Descriptors and input bytes must outlive decoded instructions. Registration
rejects assigned encodings, duplicate entries, and conflicting one/two-byte maps.
Two-byte private encodings use F0..FF escapes. A registered escape reinterprets
that whole namespace, including a formerly reserved standalone marker. Each
descriptor retains its own name, stack effects, and flow; its code is `Custom`.

## Formatting

`Instruction` and `PrefixedInstruction` implement `Display`.
`FmtFormatter` implements the object-safe `Formatter` trait and writes into any
`core::fmt::Write` sink, including `BufSink`. Buffer overflow returns `fmt::Error`
and sets `overflowed()`; previously written UTF-8 remains valid.

`FormatterOptions` controls address labels, hexadecimal case, integer formatting,
and lossless raw float-bit rendering. `SymbolResolver` optionally supplies names
for metadata tokens and branch targets; unresolved operands retain raw values.
The `alloc` feature adds `format_to_string`. `std` implies `alloc` and enables
`std::error::Error` and the CLI example:

```powershell
cargo run --features std --example disasm -- method-code.bin 0x1000
```

## Validation and development

```powershell
cargo test
cargo test --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo check --no-default-features --target wasm32-unknown-unknown
cargo check --no-default-features --target armv7a-none-eabi
python tools/generate.py --check
```

Tests cover all assigned encodings, every unused one-byte/FE/F0 slot and early map,
every truncation boundary, branch sign boundaries, float bit preservation,
large switches, cursor movement, malformed streams, prefix grouping, formatting,
and runtime dialect gates, private maps, and the six-prefix compatibility matrix.
Checked-in fixtures compare all Reflection.Emit descriptors and **3,056 actual
methods / 66,130 instructions / 40 switches** from .NET Framework 2/4 and
.NET 5/8/9/10 against an independent .NET decoder. Mono layouts also have literal
consumer/emitter regressions. Normal builds and tests need
neither Python nor .NET nor network access.

To regenerate the catalogue from the committed source snapshots:

```powershell
python tools/generate.py
cargo fmt --all
```

To regenerate the independent fixtures (requires .NET 10 SDK):

```powershell
dotnet run --project tools/oracle -- tests/data
```

The oracle also accepts an assembly path and a maximum method count. It stores
the producer runtime version and source assembly MVID next to the corpus. Commit regenerated
sources and fixtures after reviewing changes.

The dependency-free structured fuzz harness has seed/count replay. It exercises
raw and damaged method slices, prefixes, private payloads, all Mono selections,
cursor/IP invariants, switch targets, formatting, and truncation:

```powershell
cargo run --release --example stress -- 1000000 1179534676
```

Four fixed seeds also run as regression tests. This harness uses deterministic
mutation and grammar generation; it is not a coverage-guided fuzzer.

The Rust library is licensed under [MIT](LICENSE). Upstream source attribution
and reference-material notices are in [NOTICE](NOTICE). Source and issue tracking
are hosted at [valeria-org/fNET](https://github.com/valeria-org/fNET).
Version history is in [CHANGELOG.md](CHANGELOG.md); maintainers can follow the
[release instructions](https://github.com/valeria-org/fNET/blob/main/RELEASING.md).
