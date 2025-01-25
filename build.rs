fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    let lib = pkg_config::Config::new().probe("libuhdr").unwrap();
    println!("cargo::warning={lib:?}");
}