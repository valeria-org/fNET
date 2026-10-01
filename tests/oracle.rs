use fNET::*;

#[test]
fn agrees_with_independent_reflection_emit_catalogue() {
    let mut count = 0;
    for line in include_str!("data/reflection-opcodes.tsv").lines() {
        let fields: Vec<_> = line.split('\t').collect();
        let encoding = u16::from_str_radix(fields[0], 16).unwrap();
        if encoding == 0xfe {
            assert_eq!(Code::from_encoding(encoding), None);
            continue;
        }
        let code = Code::from_encoding(encoding).expect(fields[1]);
        let info = code.op_code();
        assert_eq!(info.name, fields[1]);
        assert_eq!(usize::from(info.size), fields[2].parse::<usize>().unwrap());
        assert_eq!(format!("{:?}", info.operand), fields[3], "{code:?}");
        assert_eq!(
            format!("{:?}", info.pop).to_lowercase(),
            fields[4].replace('_', "").to_lowercase(),
            "{code:?}"
        );
        assert_eq!(
            format!("{:?}", info.push).to_lowercase(),
            fields[5].replace('_', "").to_lowercase(),
            "{code:?}"
        );
        assert_eq!(
            format!("{:?}", info.flow_control).to_lowercase(),
            fields[6].replace('_', "").to_lowercase(),
            "{code:?}"
        );
        count += 1;
    }
    assert_eq!(count, 225);
    assert_eq!(Code::No.value(), 0xfe19); // ECMA-only: intentionally absent from Reflection.Emit.
}

// The fixture is generated with the .NET PEReader + Reflection.Emit opcode
// catalogue, not fNET or the separate cildec reference implementation.
#[test]
fn agrees_with_real_dotnet_runtime_methods() {
    let mut totals = (0, 0, 0);
    for bytes in [
        include_bytes!("data/runtime-corpus.bin").as_slice(),
        include_bytes!("data/framework2/runtime-corpus.bin").as_slice(),
        include_bytes!("data/framework4/runtime-corpus.bin").as_slice(),
        include_bytes!("data/net5/runtime-corpus.bin").as_slice(),
        include_bytes!("data/net8/runtime-corpus.bin").as_slice(),
        include_bytes!("data/net9/runtime-corpus.bin").as_slice(),
    ] {
        let (methods, instructions, switches) = check_corpus(bytes);
        assert!(methods >= 400);
        assert!(instructions > 9_000);
        assert!(switches > 0);
        totals.0 += methods;
        totals.1 += instructions;
        totals.2 += switches;
    }
    assert_eq!(totals, (3056, 66130, 40));
}

fn check_corpus(bytes: &[u8]) -> (usize, usize, usize) {
    let mut fixture = Fixture { bytes, offset: 0 };
    assert_eq!(fixture.take(4), b"FNET");
    let mut methods = 0;
    let mut instructions = 0;
    let mut switches = 0;
    while fixture.offset < fixture.bytes.len() {
        let rid = fixture.u32();
        let length = fixture.u32() as usize;
        let bytes = fixture.take(length);
        let count = fixture.u32();
        let mut decoder = Decoder::new(bytes, 0, DecoderOptions::ECMA335);
        for _ in 0..count {
            let offset = fixture.u32();
            let length = fixture.u32();
            let encoding = fixture.u16();
            let value = fixture.u64();
            let instruction = decoder.decode();
            assert!(!instruction.is_invalid(), "method {rid} offset {offset}");
            assert_eq!(instruction.offset(), offset as usize, "method {rid}");
            assert_eq!(
                instruction.len(),
                length as usize,
                "method {rid} offset {offset}"
            );
            assert_eq!(
                instruction.code().value(),
                encoding,
                "method {rid} offset {offset}"
            );
            let actual_value = match instruction.operand() {
                Operand::None => 0,
                Operand::Int8(v) => v as i64 as u64,
                Operand::UInt8(v) => u64::from(v),
                Operand::Int32(v) => v as i64 as u64,
                Operand::Int64(v) => v as u64,
                Operand::Float32(v) => u64::from(v),
                Operand::Float64(v) => v,
                Operand::Variable(v) => u64::from(v),
                Operand::Token(v) => u64::from(v.value()),
                Operand::Branch { target, .. } => target,
                Operand::Switch(targets) => {
                    for target in targets {
                        assert_eq!(target, u64::from(fixture.u32()));
                    }
                    switches += 1;
                    targets.len() as u64
                }
                Operand::Bytes(_) => panic!("unexpected private operand in standard fixture"),
            };
            assert_eq!(actual_value, value, "method {rid} offset {offset}");
            instructions += 1;
        }
        assert!(!decoder.can_decode(), "method {rid}");
        for grouped in Decoder::new(bytes, 0, DecoderOptions::ECMA335).into_prefixed() {
            match grouped.validate_prefixes(PrefixValidationOptions::Runtime) {
                Ok(()) | Err(PrefixValidationError::MetadataRequired(_)) => {}
                Err(error) => panic!("method {rid} offset {}: {error}", grouped.offset()),
            }
        }
        methods += 1;
    }
    (methods, instructions, switches)
}

struct Fixture<'a> {
    bytes: &'a [u8],
    offset: usize,
}
impl<'a> Fixture<'a> {
    fn take(&mut self, size: usize) -> &'a [u8] {
        let bytes = &self.bytes[self.offset..self.offset + size];
        self.offset += size;
        bytes
    }
    fn u16(&mut self) -> u16 {
        u16::from_le_bytes(self.take(2).try_into().unwrap())
    }
    fn u32(&mut self) -> u32 {
        u32::from_le_bytes(self.take(4).try_into().unwrap())
    }
    fn u64(&mut self) -> u64 {
        u64::from_le_bytes(self.take(8).try_into().unwrap())
    }
}
