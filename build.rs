// 链接 Windows 资源（app.res 内含 exe 图标，rc.exe 从 tools/app.rc 编译而来）
// 重新生成 res：
//   "C:\Program Files (x86)\Windows Kits\10\bin\<ver>\x64\rc.exe" /nologo /fo tools\app.res tools\app.rc
fn main() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let res = std::path::Path::new(&manifest).join("tools").join("app.res");
    println!("cargo:rerun-if-changed={}", res.display());
    println!("cargo:rustc-link-arg-bins={}", res.display());
}
