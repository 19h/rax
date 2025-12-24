fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    println!("cargo:rustc-link-arg=-T{}/linker.ld", manifest_dir);
    println!("cargo:rustc-link-arg=--no-pie");
    println!("cargo:rerun-if-changed=linker.ld");
}
