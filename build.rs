// Compiles native/util.c (C99) and native/util.cpp (C++17) into a static
// library that gets linked into the Rach interpreter. The Rust side declares
// these symbols via `extern "C"` blocks in src/stdlib/native.rs.

fn main() {
    cc::Build::new()
        .file("native/util.c")
        .flag_if_supported("-std=c99")
        .warnings(true)
        .compile("rach_native_c");

    cc::Build::new()
        .cpp(true)
        .file("native/util.cpp")
        .flag_if_supported("-std=c++17")
        .warnings(true)
        .compile("rach_native_cpp");

    println!("cargo:rerun-if-changed=native/util.c");
    println!("cargo:rerun-if-changed=native/util.cpp");
}
