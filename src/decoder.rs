use crate::{
    Code, DecodeError, ExtensionTable, Instruction, MetadataToken, OpCodeInfo, OpCodeKind, Operand,
    OperandKind, PrefixedInstruction, SwitchTargets,
};
use core::iter::FusedIterator;

/// Incompatible Mono F0 maps cannot be inferred from the bytes alone.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MonoDialect {
    #[default]
    Modern,
    /// Stable post-2004 map with F01B as get_last_error (before July 2019).
    Legacy,
    /// July 2002 helper map, before PROC2 was inserted at F001.
    July2002,
    /// July 2002–May 2004 helper map (maximal snapshot: March 2004).
    Early2002,
    /// May–July 2004 helper map, after FUNC2 was inserted at F001.
    May2004,
}

/// Runtime opcode dialect selection. Standard ECMA CIL is always available.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FeatureSet(u8);

impl FeatureSet {
    pub const NONE: Self = Self(0);
    pub const MONO: Self = Self(1);
    /// Modern Mono plus retired F011/F012 instructions from older runtimes.
    pub const MONO_LEGACY: Self = Self(3);
    pub const ALL: Self = Self::MONO_LEGACY;
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// Decode-time options, separate from text formatting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DecoderOptions {
    pub features: FeatureSet,
    /// Expose F8..FD and FF reserved marker descriptors as instructions.
    /// FE always starts a two-byte opcode, including when this option is set.
    pub decode_reserved: bool,
}

impl DecoderOptions {
    /// Default fARM64-style preset: all implemented extensions, no reserved bytes.
    pub const NONE: Self = Self {
        features: FeatureSet::ALL,
        decode_reserved: false,
    };
    /// Decode only standard ECMA instructions and semantic prefixes.
    pub const ECMA335: Self = Self {
        features: FeatureSet::NONE,
        decode_reserved: false,
    };
    pub const fn new() -> Self {
        Self::NONE
    }
}
impl Default for DecoderOptions {
    fn default() -> Self {
        Self::NONE
    }
}

/// Borrowing streaming decoder. Every nonempty decode consumes bytes, even on
/// error. Recognized disabled opcodes consume their complete operand; truncated
/// instructions consume the remaining tail once. No bytes means no advancement.
#[derive(Debug, Clone)]
pub struct Decoder<'a> {
    data: &'a [u8],
    position: usize,
    ip: u64,
    options: DecoderOptions,
    last_error: DecodeError,
    extensions: ExtensionTable<'a>,
    mono_dialect: MonoDialect,
}

impl<'a> Decoder<'a> {
    pub const fn new(data: &'a [u8], ip: u64, options: DecoderOptions) -> Self {
        Self {
            data,
            position: 0,
            ip,
            options,
            last_error: DecodeError::None,
            extensions: ExtensionTable::EMPTY,
            mono_dialect: MonoDialect::Modern,
        }
    }
    /// Attach an explicitly validated map for private/experimental opcode layouts.
    pub const fn with_extensions(
        data: &'a [u8],
        ip: u64,
        options: DecoderOptions,
        extensions: ExtensionTable<'a>,
    ) -> Self {
        Self {
            extensions,
            ..Self::new(data, ip, options)
        }
    }
    pub const fn with_ip(data: &'a [u8], ip: u64, options: DecoderOptions) -> Self {
        Self::new(data, ip, options)
    }
    /// Select an audited historical map explicitly; standard CIL is unaffected.
    pub const fn with_mono_dialect(mut self, dialect: MonoDialect) -> Self {
        self.mono_dialect = dialect;
        self
    }
    pub const fn mono_dialect(&self) -> MonoDialect {
        self.mono_dialect
    }
    pub const fn try_new(
        data: &'a [u8],
        ip: u64,
        options: DecoderOptions,
    ) -> Result<Self, DecodeError> {
        Ok(Self::new(data, ip, options))
    }
    pub const fn position(&self) -> usize {
        self.position
    }
    pub const fn max_position(&self) -> usize {
        self.data.len()
    }
    pub const fn ip(&self) -> u64 {
        self.ip
    }
    pub fn set_ip(&mut self, ip: u64) {
        self.ip = ip;
    }
    /// Seek while preserving the mapping between byte positions and addresses.
    /// Failed seeks leave decoder state unchanged.
    pub fn set_position(&mut self, position: usize) -> Result<(), DecodeError> {
        if position > self.data.len() {
            return Err(DecodeError::InvalidPosition);
        }
        self.ip = self
            .ip
            .wrapping_sub(self.position as u64)
            .wrapping_add(position as u64);
        self.position = position;
        self.last_error = DecodeError::None;
        Ok(())
    }
    pub const fn options(&self) -> &DecoderOptions {
        &self.options
    }
    pub const fn last_error(&self) -> DecodeError {
        self.last_error
    }
    pub const fn can_decode(&self) -> bool {
        self.position < self.data.len()
    }
    pub fn iter(&mut self) -> &mut Self {
        self
    }
    pub fn decode(&mut self) -> Instruction<'a> {
        let start = self.position;
        let ip = self.ip;
        let mut cursor = start;
        let result = self.decode_at(&mut cursor);
        let (info, operand, error) = match result {
            Ok((info, operand)) => (info, operand, DecodeError::None),
            Err(error) => {
                if error == DecodeError::TruncatedInstruction {
                    cursor = self.data.len();
                }
                (Code::Invalid.op_code(), Operand::None, error)
            }
        };
        self.position = cursor;
        self.ip = ip.wrapping_add((cursor - start) as u64);
        self.last_error = error;
        Instruction {
            code: info.code,
            info,
            operand,
            error,
            ip,
            offset: start,
            bytes: &self.data[start..cursor],
        }
    }
    pub fn decode_into(&mut self, out: &mut Instruction<'a>) {
        *out = self.decode();
    }
    pub fn decode_out(&mut self, out: &mut Instruction<'a>) {
        self.decode_into(out);
    }

    fn take(&self, cursor: &mut usize, count: usize) -> Result<&'a [u8], DecodeError> {
        let remaining = &self.data[*cursor..];
        let bytes = remaining
            .get(..count)
            .ok_or(DecodeError::TruncatedInstruction)?;
        *cursor += count;
        Ok(bytes)
    }
    fn read<const N: usize>(&self, cursor: &mut usize) -> Result<[u8; N], DecodeError> {
        let mut bytes = [0; N];
        bytes.copy_from_slice(self.take(cursor, N)?);
        Ok(bytes)
    }
    fn decode_at(&self, cursor: &mut usize) -> Result<(&'a OpCodeInfo, Operand<'a>), DecodeError> {
        if *cursor == self.data.len() {
            return Err(DecodeError::NoMoreBytes);
        }
        let first = self.read::<1>(cursor)?[0];
        let encoding = if first == 0xFE || first == 0xF0 || self.extensions.has_escape(first) {
            (u16::from(first) << 8) | u16::from(self.read::<1>(cursor)?[0])
        } else {
            u16::from(first)
        };
        let modern = || Code::from_encoding(encoding).map(Code::op_code);
        let builtin = if first == 0xF0 {
            let historical: &[OpCodeInfo] = match self.mono_dialect {
                MonoDialect::Modern => &[],
                MonoDialect::Legacy => &crate::LEGACY_MONO_OVERRIDES,
                MonoDialect::July2002 => &crate::JULY2002_MONO_OPCODES,
                MonoDialect::Early2002 => &crate::EARLY_MONO_OPCODES,
                MonoDialect::May2004 => &crate::MAY2004_MONO_OPCODES,
            };
            let found = historical.iter().find(|info| info.encoding == encoding);
            if matches!(self.mono_dialect, MonoDialect::Modern | MonoDialect::Legacy) {
                found.or_else(modern)
            } else {
                found
            }
        } else {
            modern()
        };
        let info = match builtin {
            Some(info) if !info.is_reserved() => info,
            _ => self
                .extensions
                .lookup(encoding)
                .or(builtin)
                .ok_or(DecodeError::InvalidOpcode)?,
        };
        let code = info.code;
        let operand = match info.operand {
            OperandKind::InlineNone => Operand::None,
            OperandKind::ShortInlineI => {
                let value = self.read::<1>(cursor)?[0];
                if matches!(
                    code,
                    Code::No
                        | Code::Unaligned
                        | Code::MonoFunc1
                        | Code::MonoProc2
                        | Code::MonoProc3
                        | Code::MonoFunc2
                ) {
                    Operand::UInt8(value)
                } else {
                    Operand::Int8(value as i8)
                }
            }
            OperandKind::InlineI => Operand::Int32(i32::from_le_bytes(self.read(cursor)?)),
            OperandKind::InlineI8 => Operand::Int64(i64::from_le_bytes(self.read(cursor)?)),
            OperandKind::ShortInlineR => Operand::Float32(u32::from_le_bytes(self.read(cursor)?)),
            OperandKind::InlineR => Operand::Float64(u64::from_le_bytes(self.read(cursor)?)),
            OperandKind::ShortInlineVar => Operand::Variable(u16::from(self.read::<1>(cursor)?[0])),
            OperandKind::InlineVar => Operand::Variable(u16::from_le_bytes(self.read(cursor)?)),
            OperandKind::ShortInlineBrTarget | OperandKind::InlineBrTarget => {
                let displacement = if info.operand == OperandKind::ShortInlineBrTarget {
                    self.read::<1>(cursor)?[0] as i8 as i32
                } else {
                    i32::from_le_bytes(self.read(cursor)?)
                };
                let base = self.ip.wrapping_add((*cursor - self.position) as u64);
                Operand::Branch {
                    displacement,
                    target: base.wrapping_add(displacement as i64 as u64),
                }
            }
            OperandKind::InlineSwitch => {
                let count = u32::from_le_bytes(self.read(cursor)?);
                let size = usize::try_from(count)
                    .ok()
                    .and_then(|n| n.checked_mul(4))
                    .ok_or(DecodeError::TruncatedInstruction)?;
                let bytes = self.take(cursor, size)?;
                let base_ip = self.ip.wrapping_add((*cursor - self.position) as u64);
                Operand::Switch(SwitchTargets { bytes, base_ip })
            }
            OperandKind::InlineBytes(size) => Operand::Bytes(self.take(cursor, usize::from(size))?),
            OperandKind::InlineData | OperandKind::ShortInlineData => {
                let size = if info.operand == OperandKind::ShortInlineData {
                    usize::from(self.read::<1>(cursor)?[0])
                } else {
                    usize::try_from(u32::from_le_bytes(self.read(cursor)?))
                        .map_err(|_| DecodeError::TruncatedInstruction)?
                };
                Operand::Bytes(self.take(cursor, size)?)
            }
            _ => Operand::Token(MetadataToken(u32::from_le_bytes(self.read(cursor)?))),
        };
        if info.is_reserved() && !self.options.decode_reserved {
            return Err(DecodeError::ReservedOpcode);
        }
        let required = if info.kind == OpCodeKind::MonoLegacy {
            FeatureSet::MONO_LEGACY
        } else {
            FeatureSet::MONO
        };
        if matches!(
            info.kind,
            OpCodeKind::Mono | OpCodeKind::MonoLegacy | OpCodeKind::MonoEarly
        ) && !self.options.features.contains(required)
        {
            return Err(DecodeError::DisabledExtension);
        }
        Ok((info, operand))
    }

    /// Group any number of semantic prefixes with the following instruction.
    /// Exact ordering and duplicate prefixes are preserved without validation.
    pub fn decode_prefixed(&mut self) -> PrefixedInstruction<'a> {
        let start = self.position;
        let ip = self.ip;
        let instruction = loop {
            let instruction = self.decode();
            if !instruction.is_prefix() {
                break instruction;
            }
            if !self.can_decode() {
                self.last_error = DecodeError::MissingInstruction;
                break Instruction {
                    code: Code::Invalid,
                    ip: self.ip,
                    offset: self.position,
                    bytes: &[],
                    operand: Operand::None,
                    error: self.last_error,
                    info: Code::Invalid.op_code(),
                };
            }
        };
        PrefixedInstruction {
            prefix_bytes: &self.data[start..instruction.offset],
            bytes: &self.data[start..self.position],
            instruction,
            ip,
            offset: start,
            extensions: self.extensions,
            mono_dialect: self.mono_dialect,
            options: self.options,
            following_bytes: &self.data[self.position..],
        }
    }
    /// Consume the decoder as an iterator of grouped instructions.
    pub fn into_prefixed(self) -> PrefixedIter<'a> {
        PrefixedIter { decoder: self }
    }
}

impl<'a> Iterator for Decoder<'a> {
    type Item = Instruction<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        self.can_decode().then(|| self.decode())
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        (
            usize::from(self.can_decode()),
            Some(self.data.len() - self.position),
        )
    }
}
// Decoder is intentionally not FusedIterator: seeking can make it nonempty again.

#[derive(Debug, Clone)]
pub struct PrefixedIter<'a> {
    decoder: Decoder<'a>,
}
impl<'a> Iterator for PrefixedIter<'a> {
    type Item = PrefixedInstruction<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        self.decoder
            .can_decode()
            .then(|| self.decoder.decode_prefixed())
    }
}
impl FusedIterator for PrefixedIter<'_> {}
