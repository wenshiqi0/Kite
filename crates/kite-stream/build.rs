use std::{env, fs, path::Path};

fn main() {
    let root = Path::new("native/moonlight-common-c");
    assert!(
        root.join("enet/include/enet/enet.h").exists(),
        "initialize the pinned Moonlight submodules: git submodule update --init --recursive"
    );
    let mut build = cc::Build::new();
    build
        .std("gnu11")
        .warnings(false)
        .define("NDEBUG", None)
        .define("_DEFAULT_SOURCE", None)
        .define("HAS_SOCKLEN_T", "1")
        .define("HAS_FCNTL", "1")
        .define("HAS_IOCTL", "1")
        .define("HAS_POLL", "1")
        .define("HAS_GETADDRINFO", "1")
        .define("HAS_GETNAMEINFO", "1")
        .define("HAS_INET_PTON", "1")
        .define("HAS_INET_NTOP", "1")
        .define("HAS_MSGHDR_FLAGS", "1")
        .include(root.join("src"))
        .include(root.join("enet/include"))
        .include(root.join("nanors"))
        .include(root.join("nanors/deps"))
        .include(root.join("nanors/deps/obl"))
        .include("native");
    let includes =
        env::var_os("DEP_OPENSSL_INCLUDE").expect("openssl-sys must provide its target headers");
    for include in env::split_paths(&includes) {
        build.include(include);
    }
    for entry in fs::read_dir(root.join("src")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "c") {
            build.file(path);
        }
    }
    for file in [
        "callbacks.c",
        "compress.c",
        "host.c",
        "list.c",
        "packet.c",
        "peer.c",
        "protocol.c",
        "unix.c",
        "win32.c",
    ] {
        build.file(root.join("enet").join(file));
    }
    for file in [
        "nanors/rs.c",
        "nanors/deps/obl/oblas_common.c",
        "nanors/deps/obl/oblas_lite.c",
    ] {
        build.file(root.join(file));
    }
    build
        .file("native/kite_moonlight.c")
        .compile("kite_moonlight");
    println!("cargo:rerun-if-changed=native");
}
