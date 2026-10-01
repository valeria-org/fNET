use crate::{Instruction, MetadataToken, Operand};
use core::fmt::{self, Write};

/// A pluggable formatter writing to a caller-owned sink.
pub trait Formatter {
    fn format(&self, instruction: &Instruction<'_>, output: &mut dyn Write) -> fmt::Result;
}

/// Optional metadata and label name resolution. A missing symbol uses the raw
/// token or target address. The decoder never needs a resolver.
pub trait SymbolResolver {
    fn resolve_token(&self, _token: MetadataToken) -> Option<&str> {
        None
    }
    fn resolve_branch(&self, _target: u64) -> Option<&str> {
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormatterOptions {
    pub show_address: bool,
    pub uppercase_hex: bool,
    pub hexadecimal_immediates: bool,
    /// Format IEEE floats as raw hex payloads, preserving NaNs and signed zero.
    pub float_bits: bool,
}
impl FormatterOptions {
    pub const fn new() -> Self {
        Self {
            show_address: false,
            uppercase_hex: false,
            hexadecimal_immediates: false,
            float_bits: false,
        }
    }
}
impl Default for FormatterOptions {
    fn default() -> Self {
        Self::new()
    }
}

/// IL-style text formatter with optional symbol resolution.
#[derive(Clone, Copy)]
pub struct FmtFormatter<'a> {
    options: FormatterOptions,
    resolver: Option<&'a dyn SymbolResolver>,
}
impl core::fmt::Debug for FmtFormatter<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FmtFormatter")
            .field("options", &self.options)
            .field("has_resolver", &self.resolver.is_some())
            .finish()
    }
}
impl Default for FmtFormatter<'_> {
    fn default() -> Self {
        Self::new()
    }
}
impl<'a> FmtFormatter<'a> {
    pub const fn new() -> Self {
        Self {
            options: FormatterOptions::new(),
            resolver: None,
        }
    }
    pub const fn with_options(options: FormatterOptions) -> Self {
        Self {
            options,
            resolver: None,
        }
    }
    pub const fn with_resolver(
        options: FormatterOptions,
        resolver: &'a dyn SymbolResolver,
    ) -> Self {
        Self {
            options,
            resolver: Some(resolver),
        }
    }
    pub const fn options(&self) -> &FormatterOptions {
        &self.options
    }
    pub fn options_mut(&mut self) -> &mut FormatterOptions {
        &mut self.options
    }
    fn hex(&self, output: &mut dyn Write, value: u64, width: usize) -> fmt::Result {
        if self.options.uppercase_hex {
            write!(output, "{value:0width$X}")
        } else {
            write!(output, "{value:0width$x}")
        }
    }
    fn target(&self, output: &mut dyn Write, target: u64) -> fmt::Result {
        if let Some(name) = self.resolver.and_then(|r| r.resolve_branch(target)) {
            return output.write_str(name);
        }
        output.write_str("IL_")?;
        self.hex(output, target, 4)
    }
    fn integer(&self, output: &mut dyn Write, value: i64) -> fmt::Result {
        if !self.options.hexadecimal_immediates {
            return write!(output, "{value}");
        }
        if value < 0 {
            output.write_char('-')?;
        }
        output.write_str("0x")?;
        self.hex(output, value.unsigned_abs(), 1)
    }
}
impl Formatter for FmtFormatter<'_> {
    fn format(&self, instruction: &Instruction<'_>, output: &mut dyn Write) -> fmt::Result {
        if self.options.show_address {
            self.target(output, instruction.ip())?;
            output.write_str(": ")?;
        }
        if instruction.is_invalid() {
            output.write_str("invalid")?;
            for byte in instruction.bytes() {
                output.write_char(' ')?;
                self.hex(output, u64::from(*byte), 2)?;
            }
            return Ok(());
        }
        output.write_str(instruction.name())?;
        if instruction.op_count() == 0 {
            return Ok(());
        }
        output.write_char(' ')?;
        match instruction.operand() {
            Operand::None => Ok(()),
            Operand::Int8(value) => self.integer(output, i64::from(value)),
            Operand::UInt8(value) => self.integer(output, i64::from(value)),
            Operand::Int32(value) => self.integer(output, i64::from(value)),
            Operand::Int64(value) => self.integer(output, value),
            Operand::Float32(bits) => {
                if self.options.float_bits {
                    output.write_str("float32(0x")?;
                    self.hex(output, u64::from(bits), 8)?;
                    output.write_char(')')
                } else {
                    write!(output, "{}", f32::from_bits(bits))
                }
            }
            Operand::Float64(bits) => {
                if self.options.float_bits {
                    output.write_str("float64(0x")?;
                    self.hex(output, bits, 16)?;
                    output.write_char(')')
                } else {
                    write!(output, "{}", f64::from_bits(bits))
                }
            }
            Operand::Variable(value) => write!(output, "{value}"),
            Operand::Token(token) => {
                if let Some(name) = self.resolver.and_then(|r| r.resolve_token(token)) {
                    return output.write_str(name);
                }
                output.write_str("0x")?;
                self.hex(output, u64::from(token.value()), 8)
            }
            Operand::Branch { target, .. } => self.target(output, target),
            Operand::Switch(targets) => {
                output.write_char('(')?;
                for (index, target) in targets.iter().enumerate() {
                    if index != 0 {
                        output.write_str(", ")?;
                    }
                    self.target(output, target)?;
                }
                output.write_char(')')
            }
            Operand::Bytes(bytes) => {
                output.write_char('(')?;
                for (index, byte) in bytes.iter().enumerate() {
                    if index != 0 {
                        output.write_char(' ')?;
                    }
                    self.hex(output, u64::from(*byte), 2)?;
                }
                output.write_char(')')
            }
        }
    }
}

/// A UTF-8 sink over a caller-supplied byte buffer. A write that cannot fit
/// returns `fmt::Error` and retains previously written text without partial UTF-8.
#[derive(Debug)]
pub struct BufSink<'a> {
    buffer: &'a mut [u8],
    len: usize,
    overflowed: bool,
}
impl<'a> BufSink<'a> {
    pub fn new(buffer: &'a mut [u8]) -> Self {
        Self {
            buffer,
            len: 0,
            overflowed: false,
        }
    }
    pub fn as_str(&self) -> &str {
        // Only complete UTF-8 write_str inputs are copied into this private buffer.
        core::str::from_utf8(&self.buffer[..self.len]).unwrap_or("")
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.buffer[..self.len]
    }
    pub const fn len(&self) -> usize {
        self.len
    }
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub const fn capacity(&self) -> usize {
        self.buffer.len()
    }
    pub const fn overflowed(&self) -> bool {
        self.overflowed
    }
    pub fn clear(&mut self) {
        self.len = 0;
        self.overflowed = false;
    }
}
impl Write for BufSink<'_> {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        if value.len() > self.buffer.len() - self.len {
            self.overflowed = true;
            return Err(fmt::Error);
        }
        let end = self.len + value.len();
        self.buffer[self.len..end].copy_from_slice(value.as_bytes());
        self.len = end;
        Ok(())
    }
}

#[cfg(feature = "alloc")]
pub fn format_to_string(instruction: &Instruction<'_>) -> alloc::string::String {
    use alloc::string::ToString;
    instruction.to_string()
}
