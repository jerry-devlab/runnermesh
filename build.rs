fn main() {
    println!("cargo:rerun-if-changed=resources/runnermesh-agent.rc");
    println!("cargo:rerun-if-changed=resources/runnermesh-agent.manifest");

    #[cfg(windows)]
    embed_resource::compile_for(
        "resources/runnermesh-agent.rc",
        ["runnermesh-agent"],
        embed_resource::NONE,
    )
    .manifest_required()
    .expect("the runnermesh-agent Common Controls v6 manifest must embed");
}
