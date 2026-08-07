fn main() {
    for input in [
        "1.x", "1.X", "1.*", "^1.x", "^1.*", "^1.x.y", "1.x.y", "1.2.x", "*", "1.2.*",
        "^1.2.*", "x", "1.2.3.x", ">=1.x", "1.x.3",
    ] {
        match semver::VersionReq::parse(input) {
            Ok(r) => println!("{:<10} OK   -> {r}", input),
            Err(e) => println!("{:<10} ERR  -> {e}", input),
        }
    }
}
