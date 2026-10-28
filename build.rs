use std::{env, fs, path::PathBuf};

fn main() {
    // De ieee2030_5_lib/exip-sys para ~/copel/exip
    let exip = PathBuf::from(env::var("EXIP_DIR").unwrap_or_else(|_| "../exip".into()))
        .canonicalize()
        .expect("EXIP_DIR inválido");
    let target_cfg = env::var("EXIP_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| exip.join("build/gcc/pc"));   // onde fica o exipConfig.h

    let mut includes = vec![exip.join("include"), target_cfg];
    let mut sources: Vec<PathBuf> = vec![];

    for entry in fs::read_dir(exip.join("src")).unwrap() {
        let dir = entry.unwrap().path();
        if !dir.is_dir() { continue; }
        let inc = dir.join("include");
        if inc.is_dir() { includes.push(inc); }
        let src = dir.join("src");
        if src.is_dir() {
            for f in fs::read_dir(&src).unwrap() {
                let p = f.unwrap().path();
                if p.extension().map_or(false, |e| e == "c") { sources.push(p); }
            }
        }
    }

    sources.push("csrc/sep_schema_shim.c".into());
    sources.push("csrc/sep_exi_glue.c".into());
    sources.push("csrc/sep_exi_enc.c".into());

    cc::Build::new()
        .includes(&includes)
        .files(&sources)
        .warnings(false)
        .compile("exip");

    let bindings = bindgen::Builder::default()
        .header("wrapper.h")
        .clang_args(includes.iter().map(|p| format!("-I{}", p.display())))
        .allowlist_function("sep_get_schema|initParser|parseHeader|parseNext|destroyParser|makeDefaultOpts")
        .allowlist_var("serialize")
        .allowlist_type("EXIPSchema|Parser|EXIStream|EXIOptions|BinaryBuffer|EXISerializer|QName")
        .generate()
        .expect("bindgen falhou");
    bindings
        .write_to_file(PathBuf::from(env::var("OUT_DIR").unwrap()).join("bindings.rs"))
        .unwrap();

    println!("cargo:rerun-if-changed=wrapper.h");
    println!("cargo:rerun-if-changed=csrc/sep_schema_shim.c");
    println!("cargo:rerun-if-changed=csrc/sep_exi_glue.c");
    println!("cargo:rerun-if-changed=csrc/sep_exi_enc.c");
    println!("cargo:rerun-if-env-changed=EXIP_DIR");
}
