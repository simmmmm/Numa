fn main() {
    println!("cargo::rustc-check-cfg=cfg(numa_looks)");
    println!("cargo::rerun-if-changed=looks");
    if std::path::Path::new("looks").is_dir() {
        println!("cargo::rustc-cfg=numa_looks");
    }
}
