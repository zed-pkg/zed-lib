use zed_interfaces::version::{Requirement, resolve};
fn main() {
    let versions: Vec<String> = ["1.0.0", "1.2.0", "1.2.9", "1.9.0", "2.0.0"]
        .iter().map(|s| s.to_string()).collect();
    for input in ["1.*", "1.2.*", "1.2", "1.x", "1.x.y", "*", "^1.2", "~1.2"] {
        let req = Requirement::parse(input);
        let picked = resolve(&req, &versions);
        let valid = Requirement::validate(input);
        println!("{:<8} -> {:<10?} validate={:?}", input, picked, valid.is_ok());
    }
}
