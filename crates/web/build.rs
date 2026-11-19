//! Windows（MSVC）上和桌面应用一样静态链接 VC 运行库。
//!
//! sherpa-onnx 的预编译静态库是用 /MT（静态 C 运行库）编的，Rust 默认是 /MD，两套混在一个程序里
//! 链接时会警告 LNK4098，内存在一边分配、另一边释放时可能出错。桌面应用由 tauri-build 处理
//! （tauri.conf.json 的 build.windows.staticVCRuntime 默认开着）；这里让测试、示例和网页版服务端
//! 也用同样的设置，测出来的就是用户拿到的那一套。别的系统上什么都不做。
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    static_vcruntime::metabuild();
}
