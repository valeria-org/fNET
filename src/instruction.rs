use crate::{
    Code, DecodeError, Decoder, DecoderOptions, FlowControl, MetadataToken, Mnemonic, OpCodeInfo,
    Operand, OperandKind, SwitchTargets,
};

/// A decoded value borrowing its exact bytes. No operand-sized allocation occurs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Instruction<'a> {
    pub(crate) code: Code,
    pub(crate) ip: u64,
    pub(crate) offset: usize,
    pub(crate) bytes: &'a [u8],
    pub(crate) operand: Operand<'a>,
    pub(crate) error: DecodeError,
    pub(crate) info: &'a OpCodeInfo,
}

impl Default for Instruction<'_> {
    fn default() -> Self {
        Self {
            code: Code::Invalid,
            ip: 0,
            offset: 0,
            bytes: &[],
            operand: Operand::None,
            error: DecodeError::None,
            info: Code::Invalid.op_code(),
        }
    }
}

impl<'a> Instruction<'a> {
    pub const fn code(&self) -> Code {
        self.code
    }
    pub const fn mnemonic(&self) -> Mnemonic {
        self.info.mnemonic
    }
    /// The exact encoding name, e.g. `ldc.i4.s`, rather than the normalized mnemonic.
    pub const fn name(&self) -> &'static str {
        self.info.name
    }
    pub const fn op_code(&self) -> &'a OpCodeInfo {
        self.info
    }
    /// Actual encoded identity, including registered private opcodes.
    pub const fn encoding(&self) -> u16 {
        self.info.encoding
    }
    pub const fn flow_control(&self) -> FlowControl {
        self.op_code().flow_control
    }
    pub const fn ip(&self) -> u64 {
        self.ip
    }
    pub const fn next_ip(&self) -> u64 {
        self.ip.wrapping_add(self.len() as u64)
    }
    /// Position relative to the original byte slice, independent of the IP.
    pub const fn offset(&self) -> usize {
        self.offset
    }
    /// CIL instructions can exceed 255 bytes; the length is never narrowed.
    pub const fn len(&self) -> usize {
        self.bytes.len()
    }
    pub const fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
    pub const fn bytes(&self) -> &'a [u8] {
        self.bytes
    }
    pub const fn is_invalid(&self) -> bool {
        matches!(self.code, Code::Invalid)
    }
    pub const fn is_prefix(&self) -> bool {
        self.code.is_prefix()
    }
    pub const fn error(&self) -> DecodeError {
        self.error
    }
    pub const fn operand(&self) -> Operand<'a> {
        self.operand
    }
    /// Encoded operand count: zero for macro forms, one for a whole switch table.
    pub const fn op_count(&self) -> u32 {
        if matches!(self.operand, Operand::None) {
            0
        } else {
            1
        }
    }
    pub const fn op_kind(&self, index: u32) -> Option<OperandKind> {
        if index < self.op_count() {
            Some(self.op_code().operand)
        } else {
            None
        }
    }
    pub const fn immediate_i8(&self) -> Option<i8> {
        if let Operand::Int8(value) = self.operand {
            Some(value)
        } else {
            None
        }
    }
    pub const fn immediate_u8(&self) -> Option<u8> {
        if let Operand::UInt8(value) = self.operand {
            Some(value)
        } else {
            None
        }
    }
    /// Widened signed constant, also including `ldc.i4.*` macro forms.
    pub const fn immediate_i32(&self) -> Option<i32> {
        match self.operand {
            Operand::Int8(value) => Some(value as i32),
            Operand::Int32(value) => Some(value),
            _ => self.code.implied_i32(),
        }
    }
    pub const fn immediate_i64(&self) -> Option<i64> {
        if let Operand::Int64(value) = self.operand {
            Some(value)
        } else {
            None
        }
    }
    pub const fn immediate_f32_bits(&self) -> Option<u32> {
        if let Operand::Float32(value) = self.operand {
            Some(value)
        } else {
            None
        }
    }
    pub const fn immediate_f64_bits(&self) -> Option<u64> {
        if let Operand::Float64(value) = self.operand {
            Some(value)
        } else {
            None
        }
    }
    pub fn immediate_f32(&self) -> Option<f32> {
        self.immediate_f32_bits().map(f32::from_bits)
    }
    pub fn immediate_f64(&self) -> Option<f64> {
        self.immediate_f64_bits().map(f64::from_bits)
    }
    /// Argument or local number, including implicit macro indices.
    pub const fn variable_index(&self) -> Option<u16> {
        match self.operand {
            Operand::Variable(value) => Some(value),
            _ => self.code.implied_variable(),
        }
    }
    pub const fn metadata_token(&self) -> Option<MetadataToken> {
        if let Operand::Token(value) = self.operand {
            Some(value)
        } else {
            None
        }
    }
    pub const fn near_branch_target(&self) -> Option<u64> {
        if let Operand::Branch { target, .. } = self.operand {
            Some(target)
        } else {
            None
        }
    }
    pub const fn branch_displacement(&self) -> Option<i32> {
        if let Operand::Branch { displacement, .. } = self.operand {
            Some(displacement)
        } else {
            None
        }
    }
    pub const fn switch_targets(&self) -> Option<SwitchTargets<'a>> {
        if let Operand::Switch(value) = self.operand {
            Some(value)
        } else {
            None
        }
    }
}

impl core::fmt::Display for Instruction<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        use crate::Formatter;
        crate::FmtFormatter::new().format(self, f)
    }
}

/// Ordered prefix bytes and the instruction they modify. Prefix duplicates and
/// placement are retained for analysis, without performing IL verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PrefixedInstruction<'a> {
    pub instruction: Instruction<'a>,
    pub(crate) prefix_bytes: &'a [u8],
    pub(crate) bytes: &'a [u8],
    pub(crate) ip: u64,
    pub(crate) offset: usize,
    pub(crate) extensions: crate::ExtensionTable<'a>,
    pub(crate) mono_dialect: crate::MonoDialect,
    pub(crate) options: DecoderOptions,
    pub(crate) following_bytes: &'a [u8],
}

impl<'a> PrefixedInstruction<'a> {
    pub const fn ip(&self) -> u64 {
        self.ip
    }
    pub const fn offset(&self) -> usize {
        self.offset
    }
    pub const fn len(&self) -> usize {
        self.bytes.len()
    }
    pub const fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
    pub const fn next_ip(&self) -> u64 {
        self.ip.wrapping_add(self.len() as u64)
    }
    pub const fn bytes(&self) -> &'a [u8] {
        self.bytes
    }
    pub const fn prefix_bytes(&self) -> &'a [u8] {
        self.prefix_bytes
    }
    /// Prefix instructions in source order, including exact operands and IPs.
    /// Their offsets are relative to `prefix_bytes()`, beginning at zero.
    pub fn prefixes(&self) -> Decoder<'a> {
        Decoder::with_extensions(self.prefix_bytes, self.ip, self.options, self.extensions)
            .with_mono_dialect(self.mono_dialect)
    }
}

impl core::fmt::Display for PrefixedInstruction<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        for prefix in self.prefixes() {
            write!(f, "{prefix} ")?;
        }
        write!(f, "{}", self.instruction)
    }
}
