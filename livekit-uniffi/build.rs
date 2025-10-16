use uniffi::generate_scaffolding;

fn main() {
    generate_scaffolding("src/math.udl").unwrap();
}
