//! Bettet unter Windows das Logo und Versionsinfos in die Exe ein.

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-changed=build.rs");
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "MTG Arena Builder");
        res.set("FileDescription", "MTG Arena Builder");
        if let Err(e) = res.compile() {
            // Ohne Windows-SDK fehlt nur das Icon – der Build soll trotzdem durchlaufen.
            println!("cargo:warning=Logo konnte nicht eingebettet werden: {e}");
        }
    }
}
