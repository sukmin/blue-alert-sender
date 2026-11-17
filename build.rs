//! 실행 파일에 버전 정보와 매니페스트를 넣는다.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/blue-alert-sender.manifest");

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "windows" {
        return;
    }

    let mut res = winresource::WindowsResource::new();
    // ProductName과 FileDescription은 Cargo.toml의 [package.metadata.winresource]에서 온다.
    match std::fs::read_to_string("assets/blue-alert-sender.manifest") {
        Ok(manifest) => {
            res.set_manifest(&manifest);
        }
        Err(err) => println!("cargo:warning=매니페스트를 읽지 못함: {err}"),
    }
    if let Err(err) = res.compile() {
        // rc.exe(Windows SDK)가 없어도 빌드는 되게 한다. 이때는 UI 모드에서 콘솔 창이 잠깐 보일 수 있다.
        println!("cargo:warning=Windows 리소스를 넣지 못함: {err}");
    }
}
