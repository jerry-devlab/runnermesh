fn main() {
    println!("cargo:rerun-if-env-changed=RUNNERMESH_BUILD_COMMIT");
    println!("cargo:rerun-if-env-changed=RUNNERMESH_BUILD_CHANNEL");
    println!("cargo:rerun-if-changed=resources/runnermesh-agent.rc");
    println!("cargo:rerun-if-changed=resources/runnermesh-agent.manifest");

    let commit =
        std::env::var("RUNNERMESH_BUILD_COMMIT").unwrap_or_else(|_| "development".to_owned());
    if commit != "development"
        && (commit.len() != 40
            || !commit
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)))
    {
        panic!("RUNNERMESH_BUILD_COMMIT must be an exact lowercase 40-hex commit");
    }
    let channel =
        std::env::var("RUNNERMESH_BUILD_CHANNEL").unwrap_or_else(|_| "development-test".to_owned());
    if channel.is_empty()
        || channel.len() > 64
        || !channel
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        panic!("RUNNERMESH_BUILD_CHANNEL must be a non-empty machine token");
    }
    let target = std::env::var("TARGET").expect("Cargo must provide TARGET to build.rs");
    println!("cargo:rustc-env=RUNNERMESH_BUILD_COMMIT={commit}");
    println!("cargo:rustc-env=RUNNERMESH_BUILD_CHANNEL={channel}");
    println!("cargo:rustc-env=RUNNERMESH_BUILD_TARGET={target}");

    #[cfg(windows)]
    embed_resource::compile_for(
        "resources/runnermesh-agent.rc",
        ["runnermesh-agent"],
        embed_resource::NONE,
    )
    .manifest_required()
    .expect("the runnermesh-agent Common Controls v6 manifest must embed");
}
