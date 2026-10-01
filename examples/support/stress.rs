use fNET::*;

pub struct Rng(pub u64);
impl Rng {
    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

// No runtime dependencies: replay a failing case with its original seed/count.
pub fn run(cases: usize, seed: u64) -> usize {
    let mut rng = Rng(seed.max(1));
    let descriptors = [
        OpCodeInfo::custom(0xf780, 2, "blob", OperandKind::InlineData),
        OpCodeInfo::custom(0xf781, 2, "short-blob", OperandKind::ShortInlineData),
        OpCodeInfo::custom(0xfe70, 2, "switch", OperandKind::InlineSwitch),
        OpCodeInfo::custom(0x24, 1, "wide", OperandKind::InlineBytes(16)),
    ];
    let table = ExtensionTable::new(&descriptors).unwrap();
    let mut total = 0;
    for index in 0..cases {
        let mut data = Vec::with_capacity(512);
        let count = rng.next() as usize % 256;
        match index % 5 {
            0 => data.extend((0..count).map(|_| rng.next() as u8)),
            1 => {
                for _ in 0..1 + count % 16 {
                    let info = ALL_CODES[rng.next() as usize % ALL_CODES.len()].op_code();
                    append(&mut data, info, &mut rng);
                }
            }
            2 => {
                for _ in 0..1 + count % 32 {
                    let prefix = [
                        Code::Unaligned,
                        Code::Volatile,
                        Code::Constrained,
                        Code::Readonly,
                        Code::No,
                        Code::Tail,
                    ][rng.next() as usize % 6];
                    append(&mut data, prefix.op_code(), &mut rng);
                }
                append(&mut data, Code::Callvirt.op_code(), &mut rng);
                data.push(0x2a);
            }
            3 => {
                let info = &descriptors[rng.next() as usize % descriptors.len()];
                append(&mut data, info, &mut rng);
                data.push(0x2a);
            }
            _ => {
                let fixture = include_bytes!("../../tests/data/runtime-corpus.bin");
                let start = rng.next() as usize % (fixture.len() - 256);
                data.extend_from_slice(&fixture[start..start + count]);
                for _ in 0..count % 8 {
                    let slot = rng.next() as usize % data.len().max(1);
                    if let Some(value) = data.get_mut(slot) {
                        *value ^= rng.next() as u8;
                    }
                }
            }
        }
        if index % 3 == 0 && !data.is_empty() {
            data.truncate(rng.next() as usize % data.len());
        }
        let options = if index % 3 == 0 {
            DecoderOptions::ECMA335
        } else {
            DecoderOptions {
                features: FeatureSet::ALL,
                decode_reserved: index % 2 == 0,
            }
        };
        let dialect = [
            MonoDialect::Modern,
            MonoDialect::Legacy,
            MonoDialect::July2002,
            MonoDialect::Early2002,
            MonoDialect::May2004,
        ][(index / 5) % 5];
        let extensions = if index % 2 == 0 {
            table
        } else {
            ExtensionTable::EMPTY
        };
        total += exercise(&data, rng.next(), options, dialect, extensions);
    }
    total
}

fn append(data: &mut Vec<u8>, info: &OpCodeInfo, rng: &mut Rng) {
    data.extend_from_slice(&info.encoding_bytes()[..usize::from(info.size)]);
    let size = match info.operand {
        OperandKind::InlineSwitch => {
            let count = rng.next() as u32 % 32;
            data.extend(count.to_le_bytes());
            count as usize * 4
        }
        OperandKind::InlineData => {
            let count = rng.next() as u32 % 32;
            data.extend(if rng.next() % 8 == 0 { u32::MAX } else { count }.to_le_bytes());
            count as usize
        }
        OperandKind::ShortInlineData => {
            let count = rng.next() as u8 % 32;
            data.push(count);
            usize::from(count)
        }
        operand => operand.fixed_size().unwrap(),
    };
    data.extend((0..size).map(|_| rng.next() as u8));
}

pub fn exercise(
    data: &[u8],
    ip: u64,
    options: DecoderOptions,
    dialect: MonoDialect,
    table: ExtensionTable<'_>,
) -> usize {
    let mut decoder = Decoder::with_extensions(data, ip, options, table).with_mono_dialect(dialect);
    let mut count = 0;
    while decoder.can_decode() {
        let before = decoder.position();
        let mut cloned = decoder.clone();
        let instruction = decoder.decode();
        let mut into = Instruction::default();
        cloned.decode_into(&mut into);
        assert_eq!(instruction, into);
        assert!(!instruction.is_empty());
        assert_eq!(instruction.offset(), before);
        assert_eq!(instruction.ip(), ip.wrapping_add(before as u64));
        assert_eq!(instruction.bytes(), &data[before..decoder.position()]);
        assert_eq!(instruction.next_ip(), decoder.ip());
        assert_eq!(instruction.error(), decoder.last_error());
        match instruction.operand() {
            Operand::Branch {
                displacement,
                target,
            } => assert_eq!(
                target,
                instruction
                    .next_ip()
                    .wrapping_add(displacement as i64 as u64)
            ),
            Operand::Switch(targets) => {
                assert_eq!(targets.iter().len(), targets.len());
                for (slot, target) in targets.into_iter().enumerate() {
                    assert_eq!(targets.get(slot), Some(target));
                }
                assert_eq!(targets.get(targets.len()), None);
            }
            Operand::Bytes(payload) => assert!(instruction.bytes().ends_with(payload)),
            _ => {}
        }
        let mut storage = [0u8; 64];
        let mut sink = BufSink::new(&mut storage);
        let _ = FmtFormatter::new().format(&instruction, &mut sink);
        count += 1;
    }
    assert_eq!(decoder.position(), data.len());
    assert_eq!(decoder.next(), None);
    assert!(decoder.decode().is_empty());
    let mut consumed = 0;
    for group in Decoder::with_extensions(data, ip, options, table)
        .with_mono_dialect(dialect)
        .into_prefixed()
    {
        assert!(!group.is_empty());
        assert_eq!(group.offset(), consumed);
        assert_eq!(group.bytes(), &data[consumed..consumed + group.len()]);
        assert_eq!(
            group.prefix_bytes().len() + group.instruction.len(),
            group.len()
        );
        let prefixes: usize = group
            .prefixes()
            .map(|prefix| {
                assert!(prefix.is_prefix());
                prefix.len()
            })
            .sum();
        assert_eq!(prefixes, group.prefix_bytes().len());
        let _ = group.validate_prefixes(PrefixValidationOptions::Ecma335);
        let _ = group.validate_prefixes(PrefixValidationOptions::Runtime);
        consumed += group.len();
    }
    assert_eq!(consumed, data.len());
    count
}
