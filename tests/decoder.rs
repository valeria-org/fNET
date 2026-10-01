use fNET::*;

fn sample(code: Code) -> Vec<u8> {
    let info = code.op_code();
    let mut bytes = info.encoding_bytes()[..info.size as usize].to_vec();
    match info.operand {
        OperandKind::InlineSwitch => {
            bytes.extend_from_slice(&3_u32.to_le_bytes());
            for offset in [i32::MIN, 0, i32::MAX] {
                bytes.extend_from_slice(&offset.to_le_bytes());
            }
        }
        operand => bytes.extend_from_slice(&[0xa5; 8][..operand.fixed_size().unwrap()]),
    }
    bytes
}

const EVERYTHING: DecoderOptions = DecoderOptions {
    features: FeatureSet::ALL,
    decode_reserved: true,
};

fn dialect(code: Code) -> MonoDialect {
    match code {
        Code::MonoGetLastError => MonoDialect::Legacy,
        Code::MonoFunc2 => MonoDialect::May2004,
        _ if code.op_code().kind == OpCodeKind::MonoEarly => MonoDialect::Early2002,
        _ => MonoDialect::Modern,
    }
}

#[test]
fn catalogue_and_every_assigned_encoding() {
    assert_eq!(OPCODE_COUNT, 269);
    assert_eq!(
        ALL_CODES
            .iter()
            .filter(|c| c.op_code().is_extension())
            .count(),
        43
    );
    assert_eq!(
        ALL_CODES
            .iter()
            .filter(|c| c.op_code().is_reserved())
            .count(),
        7
    );
    assert_eq!(
        ALL_CODES
            .iter()
            .filter(|c| !c.op_code().is_extension() && !c.op_code().is_reserved())
            .count(),
        219
    );
    for code in ALL_CODES {
        let info = code.op_code();
        assert_eq!(info.code, code);
        if code.value() == info.encoding {
            assert_eq!(Code::from_encoding(code.value()), Some(code));
        }
        assert_eq!(Code::from_name(info.name), Some(code));
        let bytes = sample(code);
        let dialect = dialect(code);
        let mut decoder = Decoder::new(&bytes, 0x1000, EVERYTHING).with_mono_dialect(dialect);
        let instruction = decoder.decode();
        assert_eq!(instruction.code(), code, "{code:?}");
        assert_eq!(instruction.bytes(), bytes);
        assert_eq!(instruction.ip(), 0x1000);
        assert_eq!(instruction.offset(), 0);
        assert_eq!(instruction.len(), bytes.len());
        assert_eq!(instruction.next_ip(), 0x1000 + bytes.len() as u64);
        assert_eq!(decoder.position(), bytes.len());
        assert_eq!(decoder.ip(), instruction.next_ip());
        assert_eq!(decoder.last_error(), DecodeError::None);
        assert!(!decoder.can_decode());
        assert_eq!(instruction.op_kind(1), None);
        if info.operand == OperandKind::InlineNone {
            assert_eq!(instruction.op_count(), 0);
            assert_eq!(instruction.op_kind(0), None);
        } else {
            assert_eq!(instruction.op_count(), 1);
            assert_eq!(instruction.op_kind(0), Some(info.operand));
        }
    }
    for value in 0..=u16::MAX {
        if let Some(code) = Code::from_encoding(value) {
            assert!(ALL_CODES.contains(&code));
        }
    }
    assert_eq!(Code::from_encoding(0xFE), None);
    assert_eq!(Code::from_encoding(0xFFFF), None);
}

#[test]
fn all_unassigned_byte_slots_remain_invalid() {
    for escape in [None, Some(0xFE), Some(0xF0)] {
        for value in 0..=u8::MAX {
            if escape.is_none() && matches!(value, 0xFE | 0xF0) {
                continue;
            }
            let encoding =
                escape.map_or(u16::from(value), |e| (u16::from(e) << 8) | u16::from(value));
            if Code::from_encoding(encoding).is_some() {
                continue;
            }
            let mut bytes = vec![];
            if let Some(escape) = escape {
                bytes.push(escape);
            }
            bytes.extend_from_slice(&[value, 0x2A]);
            let mut decoder = Decoder::new(&bytes, 0, EVERYTHING);
            assert!(decoder.decode().is_invalid(), "{encoding:04x}");
            assert_eq!(decoder.last_error(), DecodeError::InvalidOpcode);
            assert_eq!(decoder.position(), bytes.len() - 1);
            assert_eq!(decoder.decode().code(), Code::Ret);
        }
    }
}

#[test]
fn every_truncation_boundary_terminates_and_preserves_bytes() {
    for code in ALL_CODES {
        let bytes = sample(code);
        for length in 0..bytes.len() {
            let dialect = dialect(code);
            let mut decoder =
                Decoder::new(&bytes[..length], u64::MAX - 3, EVERYTHING).with_mono_dialect(dialect);
            let instruction = decoder.decode();
            assert!(instruction.is_invalid(), "{code:?} at {length}");
            assert_eq!(
                instruction.error(),
                if length == 0 {
                    DecodeError::NoMoreBytes
                } else {
                    DecodeError::TruncatedInstruction
                }
            );
            assert_eq!(instruction.bytes(), &bytes[..length]);
            assert_eq!(decoder.position(), length);
            assert!(!decoder.can_decode());
            assert_eq!(decoder.next(), None);
            let exhausted = decoder.decode();
            assert!(exhausted.is_empty());
            assert_eq!(exhausted.error(), DecodeError::NoMoreBytes);
        }
    }
}

#[test]
fn signed_branches_use_instruction_end_and_wrap_addresses() {
    for code in ALL_CODES {
        let short = code.op_code().operand == OperandKind::ShortInlineBrTarget;
        if !short && code.op_code().operand != OperandKind::InlineBrTarget {
            continue;
        }
        for displacement in [i32::MIN, -128, -1, 0, 1, 127, i32::MAX] {
            if short && i8::try_from(displacement).is_err() {
                continue;
            }
            let info = code.op_code();
            let mut bytes = info.encoding_bytes()[..info.size as usize].to_vec();
            if short {
                bytes.push(displacement as u8);
            } else {
                bytes.extend_from_slice(&displacement.to_le_bytes());
            }
            for ip in [0, 0x1000, u64::MAX - 1] {
                let instruction = Decoder::new(&bytes, ip, EVERYTHING).decode();
                assert_eq!(instruction.branch_displacement(), Some(displacement));
                assert_eq!(
                    instruction.near_branch_target(),
                    Some(
                        ip.wrapping_add(bytes.len() as u64)
                            .wrapping_add(displacement as i64 as u64)
                    )
                );
            }
        }
    }
}

#[test]
fn switches_are_unbounded_borrowed_tables() {
    for count in [0, 3, 64, 4096] {
        let mut bytes = vec![0x45];
        bytes.extend_from_slice(&(count as u32).to_le_bytes());
        for index in 0..count {
            bytes.extend_from_slice(&(index - 123_i32).to_le_bytes());
        }
        let instruction = Decoder::new(&bytes, u64::MAX - 100, EVERYTHING).decode();
        assert_eq!(instruction.len(), 5 + count as usize * 4);
        let targets = instruction.switch_targets().unwrap();
        assert_eq!(targets.len(), count as usize);
        assert_eq!(targets.is_empty(), count == 0);
        assert_eq!(targets.base_ip(), instruction.next_ip());
        assert_eq!(targets.as_bytes().as_ptr(), bytes[5..].as_ptr());
        for (index, target) in targets.iter().enumerate() {
            assert_eq!(
                target,
                instruction
                    .next_ip()
                    .wrapping_add((index as i64 - 123) as u64)
            );
        }
        assert_eq!(targets.get(count as usize), None);
        assert_eq!(targets.get(usize::MAX), None);
        assert_eq!(targets.iter().rev().count(), count as usize);
        let mut mixed = targets.iter();
        if count > 0 {
            assert_eq!(mixed.next(), targets.get(0));
            assert_eq!(mixed.next_back(), targets.get(count as usize - 1));
            assert_eq!(mixed.len(), count as usize - 2);
        }
    }
    let hostile = [0x45, 0xff, 0xff, 0xff, 0xff, 0x2a];
    let instruction = Decoder::new(&hostile, 0, EVERYTHING).decode();
    assert_eq!(instruction.error(), DecodeError::TruncatedInstruction);
    assert_eq!(instruction.len(), hostile.len());
}

#[test]
fn operands_preserve_signedness_tokens_and_ieee_payloads() {
    for value in [i32::MIN, -1, 0, i32::MAX] {
        let mut bytes = vec![0x20];
        bytes.extend_from_slice(&value.to_le_bytes());
        assert_eq!(
            Decoder::new(&bytes, 0, EVERYTHING).decode().immediate_i32(),
            Some(value)
        );
    }
    for value in [i64::MIN, -1, 0, i64::MAX] {
        let mut bytes = vec![0x21];
        bytes.extend_from_slice(&value.to_le_bytes());
        assert_eq!(
            Decoder::new(&bytes, 0, EVERYTHING).decode().immediate_i64(),
            Some(value)
        );
    }
    let bytes = [
        0x1f, 0xff, 0x0e, 0xff, 0xfe, 0x09, 0xff, 0xff, 0xfe, 0x12, 0xff, 0xfe, 0x19, 0xff,
    ];
    let instructions: Vec<_> = Decoder::new(&bytes, 0, EVERYTHING).collect();
    assert_eq!(instructions[0].immediate_i8(), Some(-1));
    assert_eq!(instructions[0].immediate_i32(), Some(-1));
    assert_eq!(instructions[1].variable_index(), Some(255));
    assert_eq!(instructions[2].variable_index(), Some(65535));
    assert_eq!(instructions[3].immediate_u8(), Some(255));
    assert_eq!(instructions[4].immediate_u8(), Some(255));
    for bits in [0, 0x8000_0000, 0x7f80_0000, 0x7fa1_2345, 0xffc0_0001] {
        let mut bytes = vec![0x22];
        bytes.extend_from_slice(&u32::to_le_bytes(bits));
        let instruction = Decoder::new(&bytes, 0, EVERYTHING).decode();
        assert_eq!(instruction.immediate_f32_bits(), Some(bits));
        assert_eq!(instruction.immediate_f32().unwrap().to_bits(), bits);
    }
    for bits in [
        0,
        0x8000_0000_0000_0000,
        0x7ff0_0000_0000_0000,
        0x7ff1_2345_6789_abcd,
    ] {
        let mut bytes = vec![0x23];
        bytes.extend_from_slice(&u64::to_le_bytes(bits));
        let instruction = Decoder::new(&bytes, 0, EVERYTHING).decode();
        assert_eq!(instruction.immediate_f64_bits(), Some(bits));
        assert_eq!(instruction.immediate_f64().unwrap().to_bits(), bits);
    }
    for code in ALL_CODES
        .into_iter()
        .filter(|c| c.op_code().operand.is_token())
    {
        let info = code.op_code();
        let mut bytes = info.encoding_bytes()[..info.size as usize].to_vec();
        bytes.extend_from_slice(&0x7000_0001_u32.to_le_bytes());
        let token = Decoder::new(&bytes, 0, EVERYTHING)
            .decode()
            .metadata_token()
            .unwrap();
        assert_eq!(token, MetadataToken::new(0x7000_0001));
        assert_eq!(token.kind(), TokenKind::UserString);
        assert_eq!(token.rid(), 1);
    }
    assert_eq!(MetadataToken(0xef00_0000).kind(), TokenKind::Unknown(0xef));
}

#[test]
fn macros_aliases_and_canonical_mnemonics() {
    for code in ALL_CODES {
        let bytes = sample(code);
        let instruction = Decoder::new(&bytes, 0, EVERYTHING)
            .with_mono_dialect(dialect(code))
            .decode();
        if let Some(value) = code.implied_i32() {
            assert_eq!(instruction.immediate_i32(), Some(value));
        }
        if let Some(value) = code.implied_variable() {
            assert_eq!(instruction.variable_index(), Some(value));
        }
        assert_eq!(code.canonical().mnemonic(), code.mnemonic());
        assert_eq!(code.canonical().canonical(), code.canonical());
    }
    assert_eq!(Code::LdcI4M1.mnemonic(), Mnemonic::LdcI4);
    assert_eq!(Code::BrnullS, Code::BrfalseS);
    assert_eq!(Code::Endfault, Code::Endfinally);
    assert_eq!(Code::LdelemAny, Code::Ldelem);
    assert_eq!(Code::from_name("ldc.i4.M1"), Some(Code::LdcI4M1));
    assert_eq!(Code::from_name("made-up"), None);
    assert_eq!(Code::Call.op_code().pop.count(), None);
    assert_eq!(Code::Ret.op_code().pop.count(), None);
    assert_eq!(Code::Dup.op_code().push.count(), Some(2));
    assert_eq!(Code::StelemI4.op_code().pop.count(), Some(3));
}

#[test]
fn extensions_and_reserved_markers_obey_options() {
    for code in ALL_CODES.into_iter().filter(|c| c.op_code().is_extension()) {
        let mut bytes = sample(code);
        bytes.push(0x2a);
        let dialect = dialect(code);
        let mut strict =
            Decoder::new(&bytes, 0, DecoderOptions::ECMA335).with_mono_dialect(dialect);
        let instruction = strict.decode();
        assert_eq!(instruction.error(), DecodeError::DisabledExtension);
        assert_eq!(instruction.len(), bytes.len() - 1);
        assert_eq!(strict.decode().code(), Code::Ret);
        assert_eq!(
            Decoder::new(&bytes, 0, DecoderOptions::NONE)
                .with_mono_dialect(dialect)
                .decode()
                .code(),
            code
        );
    }
    assert_eq!(Code::MonoLdptr.op_code().operand.fixed_size(), Some(4));
    assert_eq!(Code::MonoRemapOvfExc.value(), 0xf023);
    assert_eq!(Code::MonoLdvirtftnDelegate.value(), 0xf024);
    for byte in [0xf8, 0xf9, 0xfa, 0xfb, 0xfc, 0xfd, 0xff] {
        assert_eq!(
            Decoder::new(&[byte], 0, DecoderOptions::NONE)
                .decode()
                .error(),
            DecodeError::ReservedOpcode
        );
        assert!(!Decoder::new(&[byte], 0, EVERYTHING).decode().is_invalid());
    }
    assert_eq!(
        Decoder::new(&[0xfe], 0, EVERYTHING).decode().error(),
        DecodeError::TruncatedInstruction
    );
    assert_eq!(
        Decoder::new(&[0xf0, 0x11], 0, EVERYTHING).decode().error(),
        DecodeError::None
    );
    assert_eq!(
        Decoder::new(&[0xf0, 0x12], 0, EVERYTHING).decode().error(),
        DecodeError::None
    );
}

#[test]
fn cursor_decode_into_and_prefix_grouping() {
    let bytes = [
        0x00, 0xfe, 0x13, 0xfe, 0x12, 1, 0xfe, 0x16, 1, 0, 0, 2, 0x6f, 1, 0, 0, 6, 0x2a,
    ];
    let mut decoder = Decoder::new(&bytes, 0x1000, EVERYTHING);
    let mut instruction = Instruction::default();
    decoder.decode_into(&mut instruction);
    assert_eq!(instruction.code(), Code::Nop);
    let grouped = decoder.decode_prefixed();
    assert_eq!(grouped.ip(), 0x1001);
    assert_eq!(grouped.offset(), 1);
    assert_eq!(grouped.len(), 16);
    assert_eq!(grouped.instruction.code(), Code::Callvirt);
    assert_eq!(grouped.instruction.offset(), 12);
    assert_eq!(grouped.instruction.ip(), 0x100c);
    assert_eq!(
        grouped.prefixes().map(|i| i.code()).collect::<Vec<_>>(),
        [Code::Volatile, Code::Unaligned, Code::Constrained]
    );
    assert_eq!(grouped.bytes(), &bytes[1..17]);
    decoder.decode_out(&mut instruction);
    assert_eq!(instruction.code(), Code::Ret);
    assert_eq!(
        decoder.set_position(bytes.len() + 1),
        Err(DecodeError::InvalidPosition)
    );
    assert_eq!(decoder.position(), bytes.len());
    decoder.set_position(0).unwrap();
    assert_eq!(decoder.ip(), 0x1000);
    decoder.set_ip(0x2000);
    decoder.set_position(1).unwrap();
    assert_eq!(decoder.ip(), 0x2001);
    assert_eq!(decoder.iter().count(), 5);
    let mut tail = Decoder::new(&[0xfe, 0x13, 0xfe, 0x14], 0, EVERYTHING);
    let grouped = tail.decode_prefixed();
    assert_eq!(grouped.len(), 4);
    assert_eq!(grouped.prefixes().count(), 2);
    assert_eq!(grouped.instruction.error(), DecodeError::MissingInstruction);
    assert_eq!(tail.last_error(), DecodeError::MissingInstruction);
    let duplicate = [0xfe, 0x13, 0xfe, 0x13, 0x00];
    let grouped = Decoder::new(&duplicate, 0, EVERYTHING)
        .into_prefixed()
        .next()
        .unwrap();
    assert_eq!(grouped.prefixes().count(), 2);
    assert_eq!(grouped.instruction.code(), Code::Nop);
}

#[test]
fn arbitrary_streams_always_tile_and_group_the_input() {
    let mut state = 0x8765_4321_u32;
    for size in 0..512 {
        let mut bytes = vec![0; size];
        for byte in &mut bytes {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            *byte = state as u8;
        }
        let mut offset = 0;
        for instruction in Decoder::new(&bytes, 0x1000, EVERYTHING) {
            assert!(!instruction.is_empty());
            assert_eq!(instruction.offset(), offset);
            assert_eq!(instruction.ip(), 0x1000 + offset as u64);
            assert_eq!(
                instruction.bytes(),
                &bytes[offset..offset + instruction.len()]
            );
            offset += instruction.len();
            let mut output = [0; 256];
            let _ = FmtFormatter::new().format(&instruction, &mut BufSink::new(&mut output));
        }
        assert_eq!(offset, size);
        let mut offset = 0;
        for grouped in Decoder::new(&bytes, 0, EVERYTHING).into_prefixed() {
            assert!(!grouped.is_empty());
            assert_eq!(grouped.offset(), offset);
            offset += grouped.len();
        }
        assert_eq!(offset, size);
    }
}
