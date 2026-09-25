// Windows gives the main thread a 1 MB stack where macOS and Linux give 8 MB; match
// them so a deep frame that is fine elsewhere doesn't kill the game silently there.
fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg-bins=/STACK:8388608");
    }
}
