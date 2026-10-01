//! Deterministic structured fuzzing, with seed/count replay and no dependencies.
#[path = "support/stress.rs"]
mod stress;
fn main() {
    let mut args = std::env::args().skip(1);
    let cases = args
        .next()
        .map(|v| v.parse().expect("case count"))
        .unwrap_or(1_000_000);
    let seed = args
        .next()
        .map(|v| v.parse().expect("seed"))
        .unwrap_or(0x464e4554);
    let count = stress::run(cases, seed);
    println!("Passed {cases} cases / {count} raw instructions; seed {seed}");
}
