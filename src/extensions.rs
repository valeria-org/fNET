use crate::{Code, Mnemonic, OpCodeInfo, OpCodeKind};

/// Registration errors. A custom map may fill unused/reserved encodings, but
/// cannot shadow assigned standard or Mono instructions or create ambiguous maps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtensionError {
    InvalidDescriptor(u16),
    AssignedEncoding(u16),
    DuplicateEncoding(u16),
    AmbiguousEscape(u8),
}
impl core::fmt::Display for ExtensionError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "extension registration failed: {self:?}")
    }
}
#[cfg(feature = "std")]
impl std::error::Error for ExtensionError {}

/// A validated borrowing private-opcode map. Construct once and reuse it;
/// registration and lookup allocate nothing. Ordinary standard decoding retains
/// its constant-time built-in lookup and does not scan a private map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExtensionTable<'a> {
    entries: &'a [OpCodeInfo],
}
impl<'a> ExtensionTable<'a> {
    pub const EMPTY: Self = Self { entries: &[] };
    pub fn new(entries: &'a [OpCodeInfo]) -> Result<Self, ExtensionError> {
        for (index, entry) in entries.iter().enumerate() {
            let encoding = entry.encoding;
            let first = if entry.size == 1 {
                encoding as u8
            } else {
                (encoding >> 8) as u8
            };
            if entry.code != Code::Custom
                || entry.mnemonic != Mnemonic::Custom
                || entry.kind != OpCodeKind::Custom
                || entry.name.is_empty()
                || !matches!(entry.size, 1 | 2)
                || (entry.size == 1 && (encoding > 255 || matches!(first, 0xf0 | 0xfe)))
                || (entry.size == 2 && first < 0xf0)
            {
                return Err(ExtensionError::InvalidDescriptor(encoding));
            }
            if Code::from_encoding(encoding).is_some_and(|code| !code.op_code().is_reserved()) {
                return Err(ExtensionError::AssignedEncoding(encoding));
            }
            for other in &entries[..index] {
                if other.encoding == encoding {
                    return Err(ExtensionError::DuplicateEncoding(encoding));
                }
                let other_first = if other.size == 1 {
                    other.encoding as u8
                } else {
                    (other.encoding >> 8) as u8
                };
                if first == other_first && entry.size != other.size {
                    return Err(ExtensionError::AmbiguousEscape(first));
                }
            }
        }
        Ok(Self { entries })
    }
    pub const fn entries(self) -> &'a [OpCodeInfo] {
        self.entries
    }
    pub(crate) fn has_escape(self, first: u8) -> bool {
        if first < 0xf0 {
            return false;
        }
        self.entries
            .iter()
            .any(|e| e.size == 2 && (e.encoding >> 8) as u8 == first)
    }
    pub(crate) fn lookup(self, encoding: u16) -> Option<&'a OpCodeInfo> {
        self.entries.iter().find(|e| e.encoding == encoding)
    }
}
