use core::iter::FusedIterator;

/// The common token table tags. Unknown tags remain available in the raw token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenKind {
    Module,
    TypeRef,
    TypeDef,
    Field,
    MethodDef,
    MemberRef,
    StandAloneSig,
    TypeSpec,
    MethodSpec,
    UserString,
    Unknown(u8),
}

/// A raw four-byte metadata token or Mono wrapper-data index.
/// Decoding preserves all bits and does not require metadata access.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct MetadataToken(pub u32);

impl MetadataToken {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }
    pub const fn value(self) -> u32 {
        self.0
    }
    pub const fn rid(self) -> u32 {
        self.0 & 0x00FF_FFFF
    }
    pub const fn table(self) -> u8 {
        (self.0 >> 24) as u8
    }
    pub const fn kind(self) -> TokenKind {
        match self.table() {
            0x00 => TokenKind::Module,
            0x01 => TokenKind::TypeRef,
            0x02 => TokenKind::TypeDef,
            0x04 => TokenKind::Field,
            0x06 => TokenKind::MethodDef,
            0x0A => TokenKind::MemberRef,
            0x11 => TokenKind::StandAloneSig,
            0x1B => TokenKind::TypeSpec,
            0x2B => TokenKind::MethodSpec,
            0x70 => TokenKind::UserString,
            other => TokenKind::Unknown(other),
        }
    }
}

/// Decoded explicit operand. IEEE float payloads are stored as bits, retaining
/// NaN payloads and signed zero. Macro operands are exposed by Instruction helpers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Operand<'a> {
    #[default]
    None,
    Int8(i8),
    UInt8(u8),
    Int32(i32),
    Int64(i64),
    Float32(u32),
    Float64(u64),
    Variable(u16),
    Token(MetadataToken),
    Branch {
        displacement: i32,
        target: u64,
    },
    Switch(SwitchTargets<'a>),
    /// Private payload bytes, excluding any encoded count field.
    Bytes(&'a [u8]),
}

/// An allocation-free view over a validated switch displacement table.
/// All targets use the address immediately after the *entire* table as base.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SwitchTargets<'a> {
    pub(crate) bytes: &'a [u8],
    pub(crate) base_ip: u64,
}

impl<'a> SwitchTargets<'a> {
    pub const fn len(self) -> usize {
        self.bytes.len() / 4
    }
    pub const fn is_empty(self) -> bool {
        self.bytes.is_empty()
    }
    pub const fn base_ip(self) -> u64 {
        self.base_ip
    }
    /// Original table bytes, excluding the count.
    pub const fn as_bytes(self) -> &'a [u8] {
        self.bytes
    }
    pub fn displacement(self, index: usize) -> Option<i32> {
        let offset = index.checked_mul(4)?;
        let bytes = self.bytes.get(offset..)?.get(..4)?;
        Some(i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }
    pub fn get(self, index: usize) -> Option<u64> {
        self.displacement(index)
            .map(|d| self.base_ip.wrapping_add(d as i64 as u64))
    }
    pub fn iter(self) -> SwitchTargetIter<'a> {
        SwitchTargetIter {
            targets: self,
            front: 0,
            back: self.len(),
        }
    }
}

impl<'a> IntoIterator for SwitchTargets<'a> {
    type Item = u64;
    type IntoIter = SwitchTargetIter<'a>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[derive(Debug, Clone)]
pub struct SwitchTargetIter<'a> {
    targets: SwitchTargets<'a>,
    front: usize,
    back: usize,
}

impl Iterator for SwitchTargetIter<'_> {
    type Item = u64;
    fn next(&mut self) -> Option<Self::Item> {
        if self.front == self.back {
            return None;
        }
        let result = self.targets.get(self.front);
        self.front += 1;
        result
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let size = self.back - self.front;
        (size, Some(size))
    }
}
impl DoubleEndedIterator for SwitchTargetIter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.front == self.back {
            return None;
        }
        self.back -= 1;
        self.targets.get(self.back)
    }
}
impl ExactSizeIterator for SwitchTargetIter<'_> {}
impl FusedIterator for SwitchTargetIter<'_> {}
