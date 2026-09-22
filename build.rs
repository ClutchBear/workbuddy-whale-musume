// 链接 Windows 资源（app.res 内含 exe 图标）
//
// tools/app.res 是 tools/app.rc 经 Windows SDK 的 rc.exe 编译得到的二进制资源，已入库，
// 保证任何环境（含 CI / 全新 clone，即便没装 Windows SDK）都能直接构建。
//
// 改了 tools/app.rc（比如换图标）后需重新生成 app.res：
//   "C:\Program Files (x86)\Windows Kits\10\bin\<ver>\x64\rc.exe" /nologo /fo tools\app.res tools\app.rc
//
// 若 app.res 缺失：给出 cargo 警告并跳过链接（构建仍成功，只是 exe 不带图标），
// 而不是让 link.exe 抛 LNK1181 把整个构建搞崩。
use std::path::PathBuf;

fn main() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR 未设置");
    let tools = PathBuf::from(&manifest).join("tools");
    let rc = tools.join("app.rc");
    let res = tools.join("app.res");

    println!("cargo:rerun-if-changed={}", rc.display());
    println!("cargo:rerun-if-changed={}", res.display());

    if res.is_file() {
        println!("cargo:rustc-link-arg-bins={}", res.display());
    } else {
        println!(
            "cargo:warning=tools/app.res 不存在，本次构建的 exe 将不含图标；\
             请用 rc.exe 从 tools/app.rc 重新生成后再构建"
        );
    }
}
