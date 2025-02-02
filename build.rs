use bindgen::Builder;
use std::env;
use std::fs::canonicalize;
use std::path::Path;
use std::process::Command;

fn main() {
    // Following https://github.com/sujayakar/dpdk-rs/blob/main/build.rs
    // BUILD DPDK: only if the HEAD commit has changed
    //println!("cargo:rerun-if-env-changed=DPDK_PATH");
    let cargo_manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let cargo_dir = Path::new(&cargo_manifest_dir);
    // rerun if inlined.c changes
    println!(
        "cargo:rerun-if-changed={:?}",
        Path::new(&cargo_dir).join("src").join("inlined.c")
    );
    let header_path = Path::new(&cargo_dir).join("inc").join("dpdk-headers.h");
    println!("cargo:warning=Building DPDK...");
    let dpdk_path = canonicalize(cargo_dir.clone().join("dpdk")).unwrap();
    let dpdk_dir = dpdk_path.as_path();
    Command::new("./build-dpdk.sh")
        .args(&[dpdk_dir.to_str().unwrap()])
        .status()
        .unwrap_or_else(|e| panic!("Failed to build DPDK: {:?}", e));

    let dpdk_install = dpdk_dir.clone().join("install");
    let pkg_config_path = dpdk_install.join("lib/x86_64-linux-gnu/pkgconfig");

    let cflags_bytes = Command::new("pkg-config")
        .env("PKG_CONFIG_PATH", &pkg_config_path)
        .args(&["--cflags", "libdpdk"])
        .output()
        .unwrap_or_else(|e| panic!("Failed pkg-config cflags: {:?}", e))
        .stdout;
    let cflags = String::from_utf8(cflags_bytes).unwrap();

    let mut header_locations = vec![];

    for flag in cflags.split(' ') {
        if flag.starts_with("-I") {
            let header_location = &flag[2..];
            header_locations.push(header_location);
        }
    }

    // extra header for memory constants in bindings folder
    let dpdk_bindings_folder = Path::new(&cargo_dir).join("inc");
    header_locations.push(dpdk_bindings_folder.to_str().unwrap());

    let ldflags_bytes = Command::new("pkg-config")
        .env("PKG_CONFIG_PATH", &pkg_config_path)
        .args(&["--libs", "libdpdk"])
        .output()
        .unwrap_or_else(|e| panic!("Failed pkg-config ldflags: {:?}", e))
        .stdout;
    let ldflags = String::from_utf8(ldflags_bytes).unwrap();

    let mut library_location = None;
    let mut lib_names = vec![];

    for flag in ldflags.split(' ') {
        if flag.starts_with("-L") {
            library_location = Some(&flag[2..]);
        } else if flag.starts_with("-l") {
            lib_names.push(&flag[2..]);
        }
    }

    // Step 2: Now that we've compiled and installed DPDK, point cargo to the libraries.
    println!(
        "cargo:rustc-link-search=native={}",
        library_location.unwrap()
    );
    for lib_name in &lib_names {
        println!("cargo:rustc-link-lib={}", lib_name);
    }

    let mut builder = Builder::default();
    for header_location in &header_locations {
        builder = builder.clang_arg(&format!("-I{}", header_location));
    }
    let bindings = builder
        .header(header_path.to_str().unwrap())
        .blocklist_type("rte_arp_ipv4")
        .blocklist_type("rte_arp_hdr")
        .layout_tests(false)
        .generate_comments(false)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        .generate()
        .unwrap_or_else(|e| panic!("Failed to generate bindings: {:?}", e));
    let out_dir = env::var("OUT_DIR").unwrap();
    println!("out dir: {:?}", out_dir);
    let dpdk_bindings = Path::new(&out_dir).join("dpdk_bindings.rs");
    bindings
        .write_to_file(dpdk_bindings)
        .expect("Could not write bindings");

    // Compile stubs for inlined functions
    let mut compiler = cc::Build::new();
    compiler.opt_level(3);
    compiler.pic(true);
    compiler.flag("-march=native");
    compiler.flag("-Wno-unused-parameter");
    compiler.flag("-Wno-deprecated-declarations");
    let inlined_file = Path::new(&cargo_dir).join("src").join("inlined.c");
    compiler.file(inlined_file.to_str().unwrap());
    for header_location in &header_locations {
        compiler.include(header_location);
    }

    compiler.compile("inlined");
}
