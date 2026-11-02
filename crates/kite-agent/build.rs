use std::{env, path::PathBuf, process::Command};
fn main() {
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../..");
    for input in [
        "packages",
        "scripts/build-host.ts",
        "bun.lock",
        "package.json",
        "tsconfig.json",
    ] {
        println!("cargo:rerun-if-changed={}", root.join(input).display());
    }
    println!("cargo:rerun-if-env-changed=KITE_BUN");
    let bun = env::var_os("KITE_BUN").unwrap_or_else(|| "bun".into());
    let status = Command::new(bun)
        .current_dir(&root)
        .args(["run", "scripts/build-host.ts", "--out-dir"])
        .arg(env::var_os("OUT_DIR").unwrap())
        .status()
        .expect("Bun 1.3.14+ is required; run bun install --frozen-lockfile first");
    assert!(status.success(), "TypeScript host bundle build failed");
}
