use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=src/output/chafa.c");
    println!("cargo:rerun-if-env-changed=CC");
    println!("cargo:rerun-if-env-changed=PKG_CONFIG");
    let pkg = env::var("PKG_CONFIG").unwrap_or_else(|_| "pkg-config".into());
    let flags = Command::new(&pkg).args(["--cflags", "chafa >= 1.8"]).output()
        .expect("pkg-config is required to build Carbonyl with chafa");
    assert!(flags.status.success(), "Install libchafa-dev >= 1.8: {}",
            String::from_utf8_lossy(&flags.stderr));
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let object = out.join("carbonyl_chafa.o");
    let status = Command::new(env::var("CC").unwrap_or_else(|_| "cc".into()))
        .args(["-O2", "-fPIC", "-std=c11", "-Wall", "-Wextra", "-Werror"])
        .args(String::from_utf8(flags.stdout).unwrap().split_whitespace())
        .args(["-c", "src/output/chafa.c", "-o"]).arg(&object).status().unwrap();
    assert!(status.success(), "Failed to compile chafa bridge");
    assert!(Command::new("ar").arg("crs").arg(out.join("libcarbonyl_chafa.a"))
        .arg(&object).status().unwrap().success());
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=carbonyl_chafa");
    let libs = Command::new(&pkg).args(["--libs", "chafa"]).output().unwrap();
    assert!(libs.status.success());
    for flag in String::from_utf8(libs.stdout).unwrap().split_whitespace() {
        if let Some(lib) = flag.strip_prefix("-l") {
            println!("cargo:rustc-link-lib={lib}");
        } else if let Some(path) = flag.strip_prefix("-L") {
            println!("cargo:rustc-link-search=native={path}");
        }
    }
}
