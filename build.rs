fn main() {
    #[cfg(windows)]
    {
        winfsp_wrs_build::build();
        // Embed the application icon into the .exe resources.
        if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
            embed_resource::compile("assets/app.rc", embed_resource::NONE)
                .manifest_required()
                .unwrap();
        }
    }
}
