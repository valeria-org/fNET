use crate::{Code, Mnemonic};

/// Encoded operand layout, using ECMA / Reflection.Emit names.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum OperandKind {
    #[default]
    InlineNone,
    ShortInlineI,
    InlineI,
    InlineI8,
    ShortInlineR,
    InlineR,
    ShortInlineBrTarget,
    InlineBrTarget,
    InlineSwitch,
    InlineMethod,
    InlineField,
    InlineType,
    InlineTok,
    InlineString,
    InlineSig,
    ShortInlineVar,
    InlineVar,
    /// Fixed-width private data, retained as a borrowed byte slice.
    InlineBytes(u16),
    /// A u32 byte count followed by private data.
    InlineData,
    /// A u8 byte count followed by private data.
    ShortInlineData,
}

impl OperandKind {
    /// Operand byte count; a switch table has a variable length.
    pub const fn fixed_size(self) -> Option<usize> {
        Some(match self {
            Self::InlineNone => 0,
            Self::ShortInlineI | Self::ShortInlineBrTarget | Self::ShortInlineVar => 1,
            Self::InlineVar => 2,
            Self::InlineI8 | Self::InlineR => 8,
            Self::InlineSwitch | Self::InlineData | Self::ShortInlineData => return None,
            Self::InlineBytes(size) => size as usize,
            _ => 4,
        })
    }

    pub const fn is_token(self) -> bool {
        matches!(
            self,
            Self::InlineMethod
                | Self::InlineField
                | Self::InlineType
                | Self::InlineTok
                | Self::InlineString
                | Self::InlineSig
        )
    }
}

/// Static control-flow classification. `jmp` transfers control to a method.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum FlowControl {
    #[default]
    Next,
    Branch,
    CondBranch,
    Call,
    Return,
    Throw,
    Break,
    Meta,
}

/// Architectural classification of an opcode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum OpCodeKind {
    #[default]
    Primitive,
    Macro,
    ObjModel,
    Prefix,
    Reserved,
    Mono,
    MonoLegacy,
    MonoEarly,
    Custom,
}

/// Static facts for a single exact encoding. Counts describe stack values,
/// not bytes; variable call/return effects require metadata signature resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OpCodeInfo {
    pub code: Code,
    pub mnemonic: Mnemonic,
    pub name: &'static str,
    /// One-byte opcode in the low byte, or an escaped opcode such as `0xFE16`.
    pub encoding: u16,
    pub size: u8,
    pub operand: OperandKind,
    pub pop: StackBehaviourPop,
    pub push: StackBehaviourPush,
    pub flow_control: FlowControl,
    pub kind: OpCodeKind,
}

impl OpCodeInfo {
    pub const fn is_prefix(&self) -> bool {
        matches!(self.kind, OpCodeKind::Prefix)
    }
    pub const fn is_reserved(&self) -> bool {
        matches!(self.kind, OpCodeKind::Reserved)
    }
    pub const fn is_extension(&self) -> bool {
        matches!(
            self.kind,
            OpCodeKind::Mono | OpCodeKind::MonoLegacy | OpCodeKind::MonoEarly | OpCodeKind::Custom
        )
    }
    /// Create a descriptor for an explicitly registered private opcode. Change
    /// stack/flow fields with a struct update if the private semantics are known.
    pub const fn custom(encoding: u16, size: u8, name: &'static str, operand: OperandKind) -> Self {
        Self {
            code: Code::Custom,
            mnemonic: Mnemonic::Custom,
            name,
            encoding,
            size,
            operand,
            pop: StackBehaviourPop::VarPop,
            push: StackBehaviourPush::VarPush,
            flow_control: FlowControl::Next,
            kind: OpCodeKind::Custom,
        }
    }
    /// Opcode bytes in stream order. For single-byte opcodes the second is zero.
    pub const fn encoding_bytes(&self) -> [u8; 2] {
        if self.size == 1 {
            [self.encoding as u8, 0]
        } else {
            [(self.encoding >> 8) as u8, self.encoding as u8]
        }
    }
}

// The stack-behaviour enums are generated alongside the opcode catalogue.
pub use crate::generated::{StackBehaviourPop, StackBehaviourPush};
