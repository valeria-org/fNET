use fNET::*;

fn bytes(prefix: &[u8], code: Code) -> Vec<u8> {
    let info = code.op_code();
    let mut bytes = prefix.to_vec();
    bytes.extend_from_slice(&info.encoding_bytes()[..info.size as usize]);
    if info.operand == OperandKind::InlineSwitch {
        bytes.extend([0; 4]);
    } else {
        bytes.resize(bytes.len() + info.operand.fixed_size().unwrap(), 0);
    }
    bytes.push(0x2a);
    bytes
}
fn validate(bytes: &[u8]) -> Result<(), PrefixValidationError> {
    Decoder::new(bytes, 0, DecoderOptions::NONE)
        .decode_prefixed()
        .validate_prefixes(PrefixValidationOptions::Ecma335)
}

#[test]
fn memory_prefix_target_matrix_and_alignment_values() {
    use Code::*;
    let memory = [
        LdindI1, LdindU1, LdindI2, LdindU2, LdindI4, LdindU4, LdindI8, LdindI, LdindR4, LdindR8,
        LdindRef, StindRef, StindI1, StindI2, StindI4, StindI8, StindR4, StindR8, StindI, Ldfld,
        Stfld, Ldobj, Stobj, Cpblk, Initblk,
    ];
    for target in ALL_CODES.into_iter().filter(|c| {
        !c.op_code().is_prefix() && !c.op_code().is_extension() && !c.op_code().is_reserved()
    }) {
        for (prefix, expected) in [
            (&[0xfe, 0x12, 1][..], memory.contains(&target)),
            (
                &[0xfe, 0x13][..],
                memory.contains(&target) || [Ldsfld, Stsfld].contains(&target),
            ),
            (&[0xfe, 0x12, 4, 0xfe, 0x13][..], memory.contains(&target)),
            (&[0xfe, 0x13, 0xfe, 0x12, 2][..], memory.contains(&target)),
        ] {
            assert_eq!(
                validate(&bytes(prefix, target)).is_ok(),
                expected,
                "{prefix:?} {target:?}"
            );
        }
    }
    for alignment in 0..=255 {
        assert_eq!(
            validate(&bytes(&[0xfe, 0x12, alignment], LdindI4)).is_ok(),
            matches!(alignment, 1 | 2 | 4)
        );
    }
}

#[test]
fn no_prefix_checks_each_mask_bit_against_its_targets() {
    use Code::*;
    let array = [
        Ldelema, LdelemI1, LdelemU1, LdelemI2, LdelemU2, LdelemI4, LdelemU4, LdelemI8, LdelemI,
        LdelemR4, LdelemR8, LdelemRef, StelemI, StelemI1, StelemI2, StelemI4, StelemI8, StelemR4,
        StelemR8, StelemRef, Ldelem, Stelem,
    ];
    for target in ALL_CODES.into_iter().filter(|c| {
        !c.op_code().is_prefix() && !c.op_code().is_extension() && !c.op_code().is_reserved()
    }) {
        for mask in 0..=255 {
            let expected = mask != 0
                && mask & !7 == 0
                && (mask & 1 == 0
                    || [Castclass, Unbox, Ldelema, Stelem, StelemRef].contains(&target))
                && (mask & 2 == 0 || array.contains(&target))
                && (mask & 4 == 0
                    || array.contains(&target)
                    || [Ldfld, Stfld, Callvirt, Ldvirtftn].contains(&target));
            assert_eq!(
                validate(&bytes(&[0xfe, 0x19, mask], target)).is_ok(),
                expected,
                "no. {mask} {target:?}"
            );
        }
    }
}

#[test]
fn tail_and_constrained_runtime_rules() {
    for target in [Code::Call, Code::Calli, Code::Callvirt] {
        let mut stream = bytes(&[0xfe, 0x14], target);
        assert_eq!(validate(&stream), Ok(()));
        stream.pop();
        assert_eq!(
            validate(&stream),
            Err(PrefixValidationError::TailRequiresRet)
        );
        stream.push(0);
        assert_eq!(
            validate(&stream),
            Err(PrefixValidationError::TailRequiresRet)
        );
    }
    assert!(matches!(
        validate(&bytes(&[0xfe, 0x14], Code::Nop)),
        Err(PrefixValidationError::InvalidTarget { .. })
    ));
    for table in [1, 2, 0x1b] {
        for target in [Code::Call, Code::Ldftn, Code::Callvirt, Code::Nop] {
            let stream = bytes(&[0xfe, 0x16, 1, 0, 0, table], target);
            let grouped = Decoder::new(&stream, 0, DecoderOptions::NONE).decode_prefixed();
            assert_eq!(
                grouped
                    .validate_prefixes(PrefixValidationOptions::Ecma335)
                    .is_ok(),
                target == Code::Callvirt
            );
            assert_eq!(
                grouped
                    .validate_prefixes(PrefixValidationOptions::Runtime)
                    .is_ok(),
                target != Code::Nop
            );
        }
    }
    for token in [0u32, 0x02000000, 0x06000001, 0xff000001] {
        let mut prefix = vec![0xfe, 0x16];
        prefix.extend(token.to_le_bytes());
        assert_eq!(
            validate(&bytes(&prefix, Code::Callvirt)),
            Err(PrefixValidationError::InvalidOperand(Code::Constrained))
        );
    }
    let prefix = [0xfe, 0x14, 0xfe, 0x16, 1, 0, 0, 2];
    assert_eq!(validate(&bytes(&prefix, Code::Callvirt)), Ok(()));
}

#[test]
fn readonly_array_address_calls_require_resolved_metadata() {
    struct Resolver(Option<bool>);
    impl PrefixMetadataResolver for Resolver {
        fn is_array_address_method(&self, _: MetadataToken) -> Option<bool> {
            self.0
        }
    }
    assert_eq!(validate(&bytes(&[0xfe, 0x1e], Code::Ldelema)), Ok(()));
    assert!(validate(&bytes(&[0xfe, 0x1e], Code::Ldlen)).is_err());
    for target in [Code::Call, Code::Callvirt] {
        let mut stream = bytes(&[0xfe, 0x1e], target);
        stream[3..7].copy_from_slice(&0x0a000001u32.to_le_bytes());
        let grouped = Decoder::new(&stream, 0, DecoderOptions::NONE).decode_prefixed();
        assert_eq!(
            grouped.validate_prefixes(PrefixValidationOptions::Ecma335),
            Err(PrefixValidationError::MetadataRequired(MetadataToken(
                0x0a000001
            )))
        );
        for resolved in [Some(true), Some(false), None] {
            assert_eq!(
                grouped
                    .validate_prefixes_with_resolver(
                        PrefixValidationOptions::Ecma335,
                        Some(&Resolver(resolved))
                    )
                    .is_ok(),
                resolved == Some(true)
            );
        }
    }
}

#[test]
fn malformed_and_duplicate_prefixes_remain_lossless() {
    for prefix in [
        &[0xfe, 0x13][..],
        &[0xfe, 0x12, 1][..],
        &[0xfe, 0x14][..],
        &[0xfe, 0x16, 1, 0, 0, 2][..],
        &[0xfe, 0x19, 2][..],
        &[0xfe, 0x1e][..],
    ] {
        let mut duplicate = prefix.to_vec();
        duplicate.extend(prefix);
        let target = match prefix[1] {
            0x12 | 0x13 => Code::LdindI4,
            0x14 | 0x16 => Code::Callvirt,
            _ => Code::Ldelema,
        };
        let stream = bytes(&duplicate, target);
        let grouped = Decoder::new(&stream, 0, DecoderOptions::NONE).decode_prefixed();
        assert_eq!(grouped.prefixes().count(), 2);
        assert!(matches!(
            grouped.validate_prefixes(PrefixValidationOptions::Ecma335),
            Err(PrefixValidationError::DuplicatePrefix(_))
        ));
        assert_eq!(grouped.prefix_bytes(), duplicate);
        assert_eq!(
            validate(prefix),
            Err(PrefixValidationError::InvalidInstruction)
        );
    }
    for stream in [
        &[0xfe, 0x12][..],
        &[0xfe, 0x16, 1, 2][..],
        &[0xfe, 0x13, 0x24][..],
    ] {
        assert_eq!(
            validate(stream),
            Err(PrefixValidationError::InvalidInstruction)
        );
    }
    let descriptors = [OpCodeInfo::custom(
        0xf780,
        2,
        "private",
        OperandKind::InlineNone,
    )];
    let table = ExtensionTable::new(&descriptors).unwrap();
    let stream = [0xfe, 0x13, 0xf7, 0x80];
    let grouped =
        Decoder::with_extensions(&stream, 0, DecoderOptions::NONE, table).decode_prefixed();
    assert_eq!(grouped.prefixes().count(), 1);
    assert_eq!(grouped.instruction.code(), Code::Custom);
    assert!(
        grouped
            .validate_prefixes(PrefixValidationOptions::Ecma335)
            .is_err()
    );
}
