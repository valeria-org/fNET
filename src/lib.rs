//! An iced-style decoder for raw .NET CIL method code, including Mono extensions.
//!
//! The default build uses only `core`: decoding, prefix grouping, switch target
//! iteration, and formatting require no allocator. Instructions borrow their
//! original bytes, so an arbitrarily large `switch` remains a `Copy` value.
//!
//! ```
//! use fNET::{BufSink, Code, Decoder, DecoderOptions, FmtFormatter, Formatter};
//!
//! let bytes = [0x1f, 42, 0x2b, 0xfc, 0x2a];
//! let mut decoder = Decoder::new(&bytes, 0x1000, DecoderOptions::NONE);
//! let instruction = decoder.decode();
//! assert_eq!(instruction.code(), Code::LdcI4S);
//! assert_eq!(instruction.immediate_i32(), Some(42));
//! let branch = decoder.decode();
//! assert_eq!(branch.near_branch_target(), Some(0x1000));
//! let mut buffer = [0; 64];
//! let mut text = BufSink::new(&mut buffer);
//! FmtFormatter::new().format(&branch, &mut text).unwrap();
//! assert_eq!(text.as_str(), "br.s IL_1000");
//! ```
//!
//! This crate disassembles opcode streams. It does not parse PE files, resolve
//! metadata signatures, or verify stack types. Structural prefix validation is
//! available separately from permissive byte decoding.

#![no_std]
#![forbid(unsafe_code)]
#![allow(non_snake_case)]

#[cfg(feature = "alloc")]
extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

pub mod decoder;
pub mod error;
pub mod extensions;
pub mod format;
mod generated;
pub mod instruction;
pub mod op_code;
pub mod operand;
pub mod prefix;

pub use decoder::{Decoder, DecoderOptions, FeatureSet, MonoDialect, PrefixedIter};
pub use error::DecodeError;
pub use extensions::{ExtensionError, ExtensionTable};
#[cfg(feature = "alloc")]
pub use format::format_to_string;
pub use format::{BufSink, FmtFormatter, Formatter, FormatterOptions, SymbolResolver};
pub use generated::{
    ALL_CODES, Code, EARLY_MONO_OPCODES, JULY2002_MONO_OPCODES, LEGACY_MONO_OVERRIDES,
    MAY2004_MONO_OPCODES, Mnemonic, OPCODE_COUNT,
};
pub use instruction::{Instruction, PrefixedInstruction};
pub use op_code::{
    FlowControl, OpCodeInfo, OpCodeKind, OperandKind, StackBehaviourPop, StackBehaviourPush,
};
pub use operand::{MetadataToken, Operand, SwitchTargets, TokenKind};
pub use prefix::{PrefixMetadataResolver, PrefixValidationError, PrefixValidationOptions};
