use std::path::Path;

fn main() {
    let src_dir = Path::new("upstream/src");
    let parser_path = src_dir.join("parser.c");

    cc::Build::new()
        .include(src_dir)
        .file(&parser_path)
        .flag_if_supported("-Wno-unused-parameter")
        .flag_if_supported("-Wno-unused-but-set-variable")
        .flag_if_supported("-Wno-trigraphs")
        .compile("parser");

    println!("cargo:rerun-if-changed={}", parser_path.display());
    println!(
        "cargo:rerun-if-changed={}",
        src_dir.join("tree_sitter/parser.h").display()
    );
}
