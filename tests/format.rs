use core::fmt::Write;
use fNET::*;

fn text(bytes: &[u8]) -> String {
    Decoder::new(bytes, 0, DecoderOptions::NONE)
        .decode()
        .to_string()
}

#[test]
fn il_style_output_and_operand_types() {
    assert_eq!(text(&[0x00]), "nop");
    assert_eq!(text(&[0x15]), "ldc.i4.m1");
    assert_eq!(text(&[0x1f, 0xff]), "ldc.i4.s -1");
    assert_eq!(text(&[0x0e, 0xff]), "ldarg.s 255");
    assert_eq!(text(&[0xfe, 0x19, 7]), "no. 7");
    assert_eq!(text(&[0x28, 1, 0, 0, 6]), "call 0x06000001");
    assert_eq!(text(&[0x2b, 0xfe]), "br.s IL_0000");
    assert_eq!(
        text(&[0x45, 2, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff, 0xff, 0xff]),
        "switch (IL_000d, IL_000c)"
    );
    assert_eq!(text(&[0x45, 0, 0, 0, 0]), "switch ()");
    assert_eq!(text(&[0x24]), "invalid 24");
    assert_eq!(text(&[0x22, 0, 0, 0x80, 0x3f]), "ldc.r4 1");
    let grouped = Decoder::new(&[0xfe, 0x13, 0x4a], 0, DecoderOptions::NONE).decode_prefixed();
    assert_eq!(grouped.to_string(), "volatile. ldind.i4");
}

#[test]
fn sink_capacity_unicode_and_error_propagation() {
    let mut buffer = [0; 8];
    let mut sink = BufSink::new(&mut buffer);
    sink.write_str("é🦀").unwrap();
    assert_eq!(sink.len(), 6);
    assert!(sink.write_str("€").is_err());
    assert_eq!(sink.as_str(), "é🦀");
    assert!(sink.overflowed());
    sink.clear();
    assert_eq!(sink.capacity(), 8);
    assert!(sink.is_empty());
    assert!(!sink.overflowed());
    let instruction = Decoder::new(&[0x28, 1, 0, 0, 6], 0, DecoderOptions::NONE).decode();
    assert!(FmtFormatter::new().format(&instruction, &mut sink).is_err());
    assert!(sink.overflowed());
    assert!(sink.as_str().starts_with("call 0x"));
}

#[test]
fn options_and_symbol_resolution() {
    struct Resolver;
    impl SymbolResolver for Resolver {
        fn resolve_token(&self, token: MetadataToken) -> Option<&str> {
            (token.value() == 0x0600_0001).then_some("Example::Run")
        }
        fn resolve_branch(&self, target: u64) -> Option<&str> {
            (target == 0).then_some("entry")
        }
    }
    let formatter = FmtFormatter::with_resolver(FormatterOptions::default(), &Resolver);
    let mut output = String::new();
    let instruction = Decoder::new(&[0x28, 1, 0, 0, 6], 0, DecoderOptions::NONE).decode();
    formatter.format(&instruction, &mut output).unwrap();
    assert_eq!(output, "call Example::Run");
    output.clear();
    let instruction = Decoder::new(&[0x2b, 0xfe], 0, DecoderOptions::NONE).decode();
    formatter.format(&instruction, &mut output).unwrap();
    assert_eq!(output, "br.s entry");
    let formatter = FmtFormatter::with_options(FormatterOptions {
        show_address: true,
        uppercase_hex: true,
        hexadecimal_immediates: true,
        float_bits: true,
    });
    output.clear();
    let instruction = Decoder::new(&[0x1f, 0xf1], 0xabcd, DecoderOptions::NONE).decode();
    formatter.format(&instruction, &mut output).unwrap();
    assert_eq!(output, "IL_ABCD: ldc.i4.s -0xF");
    output.clear();
    let instruction =
        Decoder::new(&[0x22, 0x45, 0x23, 0xa1, 0x7f], 0, DecoderOptions::NONE).decode();
    formatter.format(&instruction, &mut output).unwrap();
    assert_eq!(output, "IL_0000: ldc.r4 float32(0x7FA12345)");
}
