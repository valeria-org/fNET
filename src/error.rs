use core::fmt;

/// Status of the most recent decode. Malformed input always makes progress.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DecodeError {
    #[default]
    None,
    /// No bytes remain. An exhausted decode returns an empty invalid instruction.
    NoMoreBytes,
    /// An escape, operand, or switch table is incomplete; the tail is consumed.
    TruncatedInstruction,
    /// The consumed opcode has no assigned encoding.
    InvalidOpcode,
    /// A reserved marker was encountered without `decode_reserved` enabled.
    ReservedOpcode,
    /// A recognized extension was disabled in the runtime feature set.
    DisabledExtension,
    /// A prefix sequence reaches the end without a following instruction.
    MissingInstruction,
    /// A requested cursor position lies beyond the byte slice.
    InvalidPosition,
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::None => "no error",
            Self::NoMoreBytes => "no more bytes",
            Self::TruncatedInstruction => "truncated instruction",
            Self::InvalidOpcode => "unassigned opcode",
            Self::ReservedOpcode => "reserved opcode",
            Self::DisabledExtension => "disabled extension",
            Self::MissingInstruction => "prefix sequence has no instruction",
            Self::InvalidPosition => "cursor position is out of bounds",
        })
    }
}

#[cfg(feature = "std")]
impl std::error::Error for DecodeError {}
