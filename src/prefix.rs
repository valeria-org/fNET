//! Optional structural prefix checks (ECMA-335 III.2). These checks do not
//! perform signature, stack, exception-region, or branch-entry verification.
use crate::{Code, MetadataToken, PrefixedInstruction, TokenKind};

/// Choose the ECMA sixth-edition rules or contemporary .NET call extensions.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrefixValidationOptions {
    #[default]
    Ecma335,
    /// Also permits constrained. call/ldftn for static interface dispatch.
    Runtime,
}

/// Metadata needed to validate readonly. calls to an array's special Address
/// method. Return None when the token cannot be resolved.
pub trait PrefixMetadataResolver {
    fn is_array_address_method(&self, token: MetadataToken) -> Option<bool>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrefixValidationError {
    InvalidInstruction,
    DuplicatePrefix(Code),
    InvalidOperand(Code),
    InvalidTarget { prefix: Code, target: Code },
    TailRequiresRet,
    MetadataRequired(MetadataToken),
}
impl core::fmt::Display for PrefixValidationError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "prefix validation failed: {self:?}")
    }
}
#[cfg(feature = "std")]
impl std::error::Error for PrefixValidationError {}

impl PrefixedInstruction<'_> {
    pub fn validate_prefixes(
        &self,
        options: PrefixValidationOptions,
    ) -> Result<(), PrefixValidationError> {
        self.validate_prefixes_with_resolver(options, None)
    }

    /// Checks values, duplicates, compatible targets, tail-call ret, and token
    /// table tags. Token existence and all full-method verifier rules are left
    /// to the caller. readonly. calls require the supplied metadata resolver.
    pub fn validate_prefixes_with_resolver(
        &self,
        options: PrefixValidationOptions,
        resolver: Option<&dyn PrefixMetadataResolver>,
    ) -> Result<(), PrefixValidationError> {
        use PrefixValidationError as Error;
        if self.instruction.is_invalid() {
            return Err(Error::InvalidInstruction);
        }
        let target = self.instruction.code();
        let mut seen = 0u8;
        for prefix in self.prefixes() {
            let code = prefix.code();
            let bit = match code {
                Code::Unaligned => 1,
                Code::Volatile => 2,
                Code::Tail => 4,
                Code::Constrained => 8,
                Code::Readonly => 16,
                Code::No => 32,
                _ => return Err(Error::InvalidInstruction),
            };
            if seen & bit != 0 {
                return Err(Error::DuplicatePrefix(code));
            }
            seen |= bit;
            let valid_target = match code {
                Code::Unaligned => {
                    if !matches!(prefix.immediate_u8(), Some(1 | 2 | 4)) {
                        return Err(Error::InvalidOperand(code));
                    }
                    indirect_memory(target)
                }
                Code::Volatile => {
                    indirect_memory(target) || matches!(target, Code::Ldsfld | Code::Stsfld)
                }
                Code::Tail => {
                    if !matches!(target, Code::Call | Code::Calli | Code::Callvirt) {
                        false
                    } else {
                        if self.following_bytes.first() != Some(&0x2a) {
                            return Err(Error::TailRequiresRet);
                        }
                        true
                    }
                }
                Code::Constrained => {
                    let token = prefix.metadata_token().ok_or(Error::InvalidOperand(code))?;
                    if token.rid() == 0
                        || !matches!(
                            token.kind(),
                            TokenKind::TypeDef | TokenKind::TypeRef | TokenKind::TypeSpec
                        )
                    {
                        return Err(Error::InvalidOperand(code));
                    }
                    target == Code::Callvirt
                        || (options == PrefixValidationOptions::Runtime
                            && matches!(target, Code::Call | Code::Ldftn))
                }
                Code::Readonly => match target {
                    Code::Ldelema => true,
                    Code::Call | Code::Callvirt => {
                        let token = self
                            .instruction
                            .metadata_token()
                            .ok_or(Error::InvalidOperand(code))?;
                        match resolver.and_then(|r| r.is_array_address_method(token)) {
                            Some(result) => result,
                            None => return Err(Error::MetadataRequired(token)),
                        }
                    }
                    _ => false,
                },
                Code::No => {
                    let mask = prefix.immediate_u8().ok_or(Error::InvalidOperand(code))?;
                    if mask == 0 || mask & !7 != 0 {
                        return Err(Error::InvalidOperand(code));
                    }
                    let array = array_access(target);
                    (mask & 1 == 0
                        || matches!(
                            target,
                            Code::Castclass
                                | Code::Unbox
                                | Code::Ldelema
                                | Code::Stelem
                                | Code::StelemRef
                        ))
                        && (mask & 2 == 0 || array)
                        && (mask & 4 == 0
                            || array
                            || matches!(
                                target,
                                Code::Ldfld | Code::Stfld | Code::Callvirt | Code::Ldvirtftn
                            ))
                }
                _ => false,
            };
            if !valid_target {
                return Err(Error::InvalidTarget {
                    prefix: code,
                    target,
                });
            }
        }
        Ok(())
    }
}

fn indirect_memory(code: Code) -> bool {
    // The one-byte indirect load/store block is contiguous in ECMA III.3.
    (0x46..=0x57).contains(&code.value())
        || matches!(
            code,
            Code::StindI
                | Code::Ldfld
                | Code::Stfld
                | Code::Ldobj
                | Code::Stobj
                | Code::Initblk
                | Code::Cpblk
        )
}
fn array_access(code: Code) -> bool {
    (0x8f..=0xa4).contains(&code.value())
}
