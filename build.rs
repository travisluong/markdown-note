fn main() {
    println!("cargo:rerun-if-changed=packaging/windows/markdown-note.rc");
    println!("cargo:rerun-if-changed=assets/brand/markdown-note.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile_for(
            "packaging/windows/markdown-note.rc",
            ["markdown-note"],
            embed_resource::NONE,
        )
        .manifest_required()
        .expect("could not embed the Markdown Note application icon");
    }
}
