fn main() {
    if cfg!(feature = "packing") {
        println!("cargo:warning=The `packing` feature is deprecated, use `writer` instead");
    }
}
