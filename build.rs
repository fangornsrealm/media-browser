fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Rebuild if i18n files change
    println!("cargo:rerun-if-changed=i18n");

    // Link libmpv (client API) for the `mpv` module.
    //
    // `mpv.so` is a system shared library (libmpv2 / mpv-libs). We link it
    // directly; if the library lives in a non-standard directory, the caller
    // can set `LIBMPV_LIB_DIR` and we add it as a search path.
    if cfg!(any(target_os = "linux", target_os = "macos")) {
        if let Some(lib_dir) = std::env::var_os("LIBMPV_LIB_DIR") {
            println!("cargo:rustc-link-search=native={lib_dir:?}");
        }
        println!("cargo:rustc-link-lib=dylib=mpv");
    }

    // Emit version information (if not cached by just vendor)
    let mut vergen = vergen::EmitBuilder::builder();
    println!("cargo:rerun-if-env-changed=VERGEN_GIT_COMMIT_DATE");
    if std::env::var_os("VERGEN_GIT_COMMIT_DATE").is_none() {
        vergen.git_commit_date();
    }
    println!("cargo:rerun-if-env-changed=VERGEN_GIT_SHA");
    if std::env::var_os("VERGEN_GIT_SHA").is_none() {
        vergen.git_sha(false);
    }
    vergen.fail_on_error().emit()?;
    Ok(())
}
