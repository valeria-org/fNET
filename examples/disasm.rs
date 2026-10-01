use fNET::{Decoder, DecoderOptions, FmtFormatter, Formatter, FormatterOptions, MonoDialect};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!(
            "Usage: cargo run --features std --example disasm -- <raw-cil.bin> [hex-base-ip] [modern|legacy|july2002|early2002|may2004]"
        );
        std::process::exit(2);
    };
    let ip = args
        .next()
        .map(|value| u64::from_str_radix(value.trim_start_matches("0x"), 16))
        .transpose()?
        .unwrap_or(0);
    let dialect = match args.next().as_deref().unwrap_or("modern") {
        "modern" => MonoDialect::Modern,
        "legacy" => MonoDialect::Legacy,
        "july2002" => MonoDialect::July2002,
        "early2002" => MonoDialect::Early2002,
        "may2004" => MonoDialect::May2004,
        _ => return Err("unknown Mono dialect".into()),
    };
    let bytes = std::fs::read(path)?;
    let formatter = FmtFormatter::with_options(FormatterOptions {
        show_address: true,
        ..FormatterOptions::default()
    });
    for instruction in Decoder::new(&bytes, ip, DecoderOptions::NONE).with_mono_dialect(dialect) {
        let mut text = String::new();
        formatter.format(&instruction, &mut text)?;
        if instruction.is_invalid() {
            println!("{text} ; {}", instruction.error());
        } else {
            println!("{text}");
        }
    }
    Ok(())
}
