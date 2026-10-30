use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

const EXIP_REPOSITORY: &str = "https://github.com/ekrich/exip.git";
const EXIP_REVISION: &str = "7bdf82198f396ddc48a2b16e40eba6650c38c0c2";

fn git<I, S>(args: I, description: &str) -> std::process::Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let output = Command::new("git")
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("failed to run git while {description}: {e}"));
    if !output.status.success() {
        panic!(
            "git failed while {description}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    output
}

fn exip_source(out_dir: &Path) -> PathBuf {
    if let Some(path) = env::var_os("EXIP_DIR") {
        return PathBuf::from(path)
            .canonicalize()
            .expect("EXIP_DIR must point to an existing EXIP source directory");
    }

    let path = out_dir.join("exip-src");
    if !path.exists() {
        git(
            ["clone", "--no-checkout", EXIP_REPOSITORY, path.to_str().expect("invalid EXIP cache path")],
            "cloning EXIP",
        );
    }

    git(
        [
            "-C",
            path.to_str().expect("invalid EXIP cache path"),
            "checkout",
            "--detach",
            EXIP_REVISION,
        ],
        "checking out the pinned EXIP revision",
    );

    let revision = git(
        [
            "-C",
            path.to_str().expect("invalid EXIP cache path"),
            "rev-parse",
            "HEAD",
        ],
        "verifying the EXIP revision",
    );
    let revision = String::from_utf8_lossy(&revision.stdout);
    assert_eq!(
        revision.trim(),
        EXIP_REVISION,
        "the EXIP source does not match the pinned revision"
    );

    path.canonicalize()
        .expect("failed to resolve the EXIP source directory")
}

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR is not set"));
    let exip = exip_source(&out_dir);
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
    println!("cargo:rerun-if-env-changed=EXIP_TARGET_DIR");
}
