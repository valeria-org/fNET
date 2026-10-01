use fNET::*;

#[test]
fn early_mono_conflicting_map_is_explicit_and_unsigned() {
    // Independent consumer/emitter layouts at Mono d5f3725 (2002), rather than
    // widths derived from fNET's descriptor table.
    let cases: &[(u8, Code, &[u8])] = &[
        (0, Code::MonoFunc1, &[0xff]),
        (1, Code::MonoProc2, &[0x80]),
        (2, Code::MonoProc3, &[0x7f]),
        (3, Code::MonoFree, &[]),
        (4, Code::MonoObjaddr, &[]),
        (5, Code::MonoLdptr, &[0x78, 0x56, 0x34, 0x12]),
        (6, Code::MonoVtaddr, &[]),
        (7, Code::MonoNewobj, &[1, 0, 0, 2]),
        (8, Code::MonoRetobj, &[2, 0, 0, 2]),
        (9, Code::MonoLdnativeobj, &[3, 0, 0, 2]),
        (10, Code::MonoCisinst, &[4, 0, 0, 2]),
        (11, Code::MonoCcastclass, &[5, 0, 0, 2]),
    ];
    for &(slot, code, payload) in cases {
        let bytes: Vec<_> = [0xf0, slot]
            .into_iter()
            .chain(payload.iter().copied())
            .chain([0x2a])
            .collect();
        let mut decoder =
            Decoder::new(&bytes, 0, DecoderOptions::NONE).with_mono_dialect(MonoDialect::Early2002);
        let instruction = decoder.decode();
        assert_eq!(instruction.code(), code);
        assert_eq!(instruction.encoding(), 0xf000 | u16::from(slot));
        assert_eq!(instruction.len(), 2 + payload.len());
        assert_eq!(instruction.op_code().kind, OpCodeKind::MonoEarly);
        if slot <= 2 {
            assert_eq!(instruction.immediate_u8(), Some(payload[0]));
        }
        assert_eq!(decoder.decode().code(), Code::Ret);
        for end in 1..instruction.len() {
            let value = Decoder::new(&bytes[..end], 0, DecoderOptions::NONE)
                .with_mono_dialect(MonoDialect::Early2002)
                .decode();
            assert_eq!(value.error(), DecodeError::TruncatedInstruction);
            assert_eq!(value.len(), end);
        }
        let value = Decoder::new(&bytes, 0, DecoderOptions::ECMA335)
            .with_mono_dialect(MonoDialect::Early2002)
            .decode();
        assert_eq!(value.error(), DecodeError::DisabledExtension);
        assert_eq!(value.len(), 2 + payload.len());
    }
    assert_eq!(
        Decoder::new(&[0xf0, 0, 0, 0, 0, 0], 0, DecoderOptions::NONE)
            .decode()
            .code(),
        Code::MonoIcall
    );
    assert_eq!(
        Decoder::new(&[0xf0, 0x24], 0, DecoderOptions::NONE)
            .with_mono_dialect(MonoDialect::Early2002)
            .decode()
            .error(),
        DecodeError::InvalidOpcode
    );
    assert_eq!(EARLY_MONO_OPCODES.len(), 12);
}

#[test]
fn retired_mono_and_consumer_metadata_corrections() {
    for (slot, code) in [(0x11, Code::MonoJitAttach), (0x12, Code::MonoJitDetach)] {
        let bytes = [0xf0, slot, 0x2a];
        let modern = DecoderOptions {
            features: FeatureSet::MONO,
            decode_reserved: false,
        };
        let mut decoder = Decoder::new(&bytes, 0, modern);
        assert_eq!(decoder.decode().error(), DecodeError::DisabledExtension);
        assert_eq!(decoder.decode().code(), Code::Ret);
        assert_eq!(
            Decoder::new(&bytes, 0, DecoderOptions::NONE)
                .decode()
                .code(),
            code
        );
    }
    // Explicit historical emitter layouts, including incorrect upstream table
    // declarations for icall, dyn_call, jit_icall_addr, and memory_barrier.
    for (slot, width) in [(0, 6), (0xf, 2), (0x10, 6), (0x13, 6), (7, 6), (8, 6)] {
        let mut bytes = vec![0xf0, slot];
        bytes.resize(width, 0);
        bytes.push(0x2a);
        let mut decoder = Decoder::new(&bytes, 0, DecoderOptions::NONE);
        assert_eq!(decoder.decode().len(), width);
        assert_eq!(decoder.decode().code(), Code::Ret);
    }
    assert_eq!(Code::MonoDynCall.op_code().pop.count(), Some(2));
    assert_eq!(Code::MonoDynCall.op_code().push.count(), Some(0));
    assert_eq!(
        Code::MonoLdnativeobj.op_code().flow_control,
        FlowControl::Next
    );
    assert_eq!(Code::MonoDynCall.op_code().flow_control, FlowControl::Call);
    assert_eq!(Code::MonoIcall.op_code().flow_control, FlowControl::Call);
}

#[test]
fn historical_insertion_maps_cover_all_assigned_and_unused_slots() {
    use Code::*;
    let july = [
        (MonoFunc1, 3),
        (MonoProc3, 3),
        (MonoFree, 2),
        (MonoObjaddr, 2),
        (MonoLdptr, 6),
        (MonoVtaddr, 2),
        (MonoNewobj, 6),
    ];
    let may = [
        (MonoFunc1, 3),
        (MonoFunc2, 3),
        (MonoProc2, 3),
        (MonoProc3, 3),
        (MonoFree, 2),
        (MonoObjaddr, 2),
        (MonoLdptr, 6),
        (MonoVtaddr, 2),
        (MonoNewobj, 6),
        (MonoRetobj, 6),
        (MonoLdnativeobj, 6),
        (MonoCisinst, 6),
        (MonoCcastclass, 6),
        (MonoSaveLmf, 2),
        (MonoRestoreLmf, 2),
    ];
    let early = [
        (MonoFunc1, 3),
        (MonoProc2, 3),
        (MonoProc3, 3),
        (MonoFree, 2),
        (MonoObjaddr, 2),
        (MonoLdptr, 6),
        (MonoVtaddr, 2),
        (MonoNewobj, 6),
        (MonoRetobj, 6),
        (MonoLdnativeobj, 6),
        (MonoCisinst, 6),
        (MonoCcastclass, 6),
    ];
    for (dialect, layout) in [
        (MonoDialect::July2002, july.as_slice()),
        (MonoDialect::Early2002, early.as_slice()),
        (MonoDialect::May2004, may.as_slice()),
    ] {
        for (slot, &(code, width)) in layout.iter().enumerate() {
            let mut bytes = vec![0xf0, slot as u8];
            bytes.resize(width, 0xff);
            bytes.push(0x2a);
            let mut decoder =
                Decoder::new(&bytes, 0, DecoderOptions::NONE).with_mono_dialect(dialect);
            let instruction = decoder.decode();
            assert_eq!(instruction.code(), code, "{dialect:?} {slot}");
            assert_eq!(instruction.encoding(), 0xf000 | slot as u16);
            assert_eq!(instruction.len(), width);
            assert_eq!(decoder.decode().code(), Ret);
            if matches!(code, MonoFunc1 | MonoFunc2 | MonoProc2 | MonoProc3) {
                assert_eq!(instruction.immediate_u8(), Some(255));
            }
            for end in 1..width {
                assert_eq!(
                    Decoder::new(&bytes[..end], 0, DecoderOptions::NONE)
                        .with_mono_dialect(dialect)
                        .decode()
                        .error(),
                    DecodeError::TruncatedInstruction
                );
            }
            let grouped = Decoder::new(&bytes, 0, DecoderOptions::NONE)
                .with_mono_dialect(dialect)
                .decode_prefixed();
            assert_eq!(grouped.instruction, instruction);
        }
        for slot in layout.len()..=255 {
            let bytes = [0xf0, slot as u8, 0x2a];
            let mut decoder =
                Decoder::new(&bytes, 0, DecoderOptions::NONE).with_mono_dialect(dialect);
            assert_eq!(decoder.decode().error(), DecodeError::InvalidOpcode);
            assert_eq!(decoder.decode().code(), Ret);
        }
    }
    assert_eq!(JULY2002_MONO_OPCODES.len(), 7);
    assert_eq!(MAY2004_MONO_OPCODES.len(), 15);
}

#[test]
fn legacy_last_error_has_a_distinct_identity_and_stack_effect() {
    let bytes = [0xf0, 0x1b, 0x2a];
    let modern = Decoder::new(&bytes, 0, DecoderOptions::NONE).decode();
    let mut decoder =
        Decoder::new(&bytes, 0, DecoderOptions::NONE).with_mono_dialect(MonoDialect::Legacy);
    let legacy = decoder.decode();
    assert_eq!(modern.code(), Code::MonoSaveLastError);
    assert_eq!(legacy.code(), Code::MonoGetLastError);
    assert_eq!(legacy.encoding(), modern.encoding());
    assert_eq!(legacy.len(), 2);
    assert_eq!(legacy.op_code().push.count(), Some(1));
    assert_eq!(modern.op_code().push.count(), Some(0));
    assert_eq!(legacy.to_string(), "mono_get_last_error");
    assert_eq!(decoder.decode().code(), Code::Ret);
    assert_eq!(
        Code::from_name("mono_get_last_error"),
        Some(Code::MonoGetLastError)
    );
    let options = DecoderOptions {
        features: FeatureSet::MONO,
        decode_reserved: false,
    };
    assert_eq!(
        Decoder::new(&bytes, 0, options)
            .with_mono_dialect(MonoDialect::Legacy)
            .decode()
            .error(),
        DecodeError::DisabledExtension
    );
}

#[test]
fn private_maps_cover_fixed_variable_branch_and_switch_payloads() {
    let cases = [
        (OperandKind::InlineNone, vec![]),
        (OperandKind::ShortInlineI, vec![0xff]),
        (OperandKind::InlineI, vec![1, 2, 3, 4]),
        (OperandKind::InlineI8, vec![1; 8]),
        (OperandKind::ShortInlineR, vec![0; 4]),
        (OperandKind::InlineR, vec![0; 8]),
        (OperandKind::InlineVar, vec![0xff; 2]),
        (OperandKind::ShortInlineVar, vec![0xff]),
        (OperandKind::InlineMethod, vec![1, 0, 0, 6]),
        (OperandKind::InlineBrTarget, (-6i32).to_le_bytes().to_vec()),
        (OperandKind::ShortInlineBrTarget, vec![0xfd]),
        (
            OperandKind::InlineSwitch,
            vec![1, 0, 0, 0, 0xf6, 0xff, 0xff, 0xff],
        ),
        (OperandKind::InlineBytes(3), vec![0xaa, 0xbb, 0xcc]),
        (OperandKind::InlineData, vec![3, 0, 0, 0, 0xaa, 0xbb, 0xcc]),
        (OperandKind::ShortInlineData, vec![3, 0xaa, 0xbb, 0xcc]),
        (OperandKind::InlineData, vec![0; 4]),
        (OperandKind::ShortInlineData, vec![0]),
    ];
    for (kind, payload) in cases {
        let descriptors = [OpCodeInfo::custom(0xf780, 2, "private", kind)];
        let table = ExtensionTable::new(&descriptors).unwrap();
        let mut bytes = vec![0xf7, 0x80];
        bytes.extend_from_slice(&payload);
        bytes.push(0x2a);
        let mut decoder = Decoder::with_extensions(&bytes, 0x1000, DecoderOptions::ECMA335, table);
        let instruction = decoder.decode();
        assert_eq!(instruction.code(), Code::Custom);
        assert_eq!(instruction.encoding(), 0xf780);
        assert_eq!(instruction.name(), "private");
        assert_eq!(instruction.op_code(), &descriptors[0]);
        assert_eq!(instruction.len(), bytes.len() - 1);
        match kind {
            OperandKind::InlineBytes(_)
            | OperandKind::InlineData
            | OperandKind::ShortInlineData => {
                let Operand::Bytes(data) = instruction.operand() else {
                    panic!()
                };
                assert_eq!(
                    data.len(),
                    if payload.iter().all(|v| *v == 0) {
                        0
                    } else {
                        3
                    }
                );
                assert!(instruction.to_string().starts_with("private ("));
            }
            OperandKind::InlineBrTarget | OperandKind::ShortInlineBrTarget => {
                assert_eq!(instruction.near_branch_target(), Some(0x1000))
            }
            OperandKind::InlineSwitch => {
                assert_eq!(instruction.switch_targets().unwrap().get(0), Some(0x1000))
            }
            _ => {}
        }
        assert_eq!(decoder.decode().code(), Code::Ret);
        for end in 1..bytes.len() - 1 {
            let value =
                Decoder::with_extensions(&bytes[..end], 0, DecoderOptions::NONE, table).decode();
            assert_eq!(
                value.error(),
                DecodeError::TruncatedInstruction,
                "{kind:?} end={end}"
            );
            assert_eq!(value.bytes(), &bytes[..end]);
        }
    }
    for encoding in [0x24, 0xf8] {
        let descriptors = [OpCodeInfo::custom(
            encoding,
            1,
            "private",
            OperandKind::InlineNone,
        )];
        let table = ExtensionTable::new(&descriptors).unwrap();
        assert_eq!(
            Decoder::with_extensions(&[encoding as u8], 0, DecoderOptions::NONE, table)
                .decode()
                .code(),
            Code::Custom
        );
    }
}

#[test]
fn private_registration_rejects_ambiguous_and_assigned_encodings() {
    let make =
        |encoding, size| OpCodeInfo::custom(encoding, size, "private", OperandKind::InlineNone);
    for entry in [
        make(0xfe, 1),
        make(0xf0, 1),
        make(0xff80, 1),
        make(0x2480, 2),
        make(0xf780, 3),
        OpCodeInfo {
            name: "",
            ..make(0xf780, 2)
        },
        *Code::Nop.op_code(),
    ] {
        assert!(matches!(
            ExtensionTable::new(&[entry]),
            Err(ExtensionError::InvalidDescriptor(_))
        ));
    }
    for encoding in [0, 0xfe16, 0xf000, 0xf011] {
        assert_eq!(
            ExtensionTable::new(&[make(encoding, if encoding > 255 { 2 } else { 1 })]),
            Err(ExtensionError::AssignedEncoding(encoding))
        );
    }
    assert_eq!(
        ExtensionTable::new(&[make(0xf780, 2), make(0xf780, 2)]),
        Err(ExtensionError::DuplicateEncoding(0xf780))
    );
    for entries in [
        [make(0xf8, 1), make(0xf880, 2)],
        [make(0xf880, 2), make(0xf8, 1)],
    ] {
        assert_eq!(
            ExtensionTable::new(&entries),
            Err(ExtensionError::AmbiguousEscape(0xf8))
        );
    }
    let descriptors = [make(0xf880, 2)];
    let table = ExtensionTable::new(&descriptors).unwrap();
    assert_eq!(
        Decoder::with_extensions(&[0xf8], 0, DecoderOptions::NONE, table)
            .decode()
            .error(),
        DecodeError::TruncatedInstruction
    );
    assert_eq!(
        Decoder::with_extensions(&[0xf8, 0x81], 0, DecoderOptions::NONE, table)
            .decode()
            .error(),
        DecodeError::InvalidOpcode
    );
    let huge = [OpCodeInfo::custom(
        0xf780,
        2,
        "blob",
        OperandKind::InlineData,
    )];
    let table = ExtensionTable::new(&huge).unwrap();
    assert_eq!(
        Decoder::with_extensions(
            &[0xf7, 0x80, 0xff, 0xff, 0xff, 0xff],
            0,
            DecoderOptions::NONE,
            table
        )
        .decode()
        .error(),
        DecodeError::TruncatedInstruction
    );
}

#[test]
fn large_private_payloads_are_borrowed_without_narrowing_lengths() {
    for kind in [OperandKind::InlineBytes(u16::MAX), OperandKind::InlineData] {
        let descriptors = [OpCodeInfo::custom(0xf780, 2, "large", kind)];
        let table = ExtensionTable::new(&descriptors).unwrap();
        let length = if kind == OperandKind::InlineData {
            70_000
        } else {
            usize::from(u16::MAX)
        };
        let mut bytes = vec![0xf7, 0x80];
        if kind == OperandKind::InlineData {
            bytes.extend((length as u32).to_le_bytes());
        }
        let payload_start = bytes.len();
        bytes.resize(payload_start + length, 0xaa);
        bytes.push(0x2a);
        let mut decoder = Decoder::with_extensions(&bytes, 0, DecoderOptions::NONE, table);
        let instruction = decoder.decode();
        let Operand::Bytes(payload) = instruction.operand() else {
            panic!()
        };
        assert_eq!(payload.len(), length);
        assert_eq!(payload.as_ptr(), bytes[payload_start..].as_ptr());
        assert_eq!(instruction.len(), length + payload_start);
        assert_eq!(decoder.decode().code(), Code::Ret);
    }
}
