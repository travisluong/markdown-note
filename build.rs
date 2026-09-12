fn main() {
    println!("cargo:rerun-if-changed=packaging/windows/mithril.rc");
    println!("cargo:rerun-if-changed=assets/brand/mithril.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile_for(
            "packaging/windows/mithril.rc",
            ["mithril"],
            embed_resource::NONE,
        )
        .manifest_required()
        .expect("could not embed the Mithril application icon");
    }
}
