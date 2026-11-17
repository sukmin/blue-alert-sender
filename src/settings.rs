//! 마지막으로 쓴 수신기 주소를 기억한다 (`%LOCALAPPDATA%\blue-alert-sender\address.txt`).

use std::path::PathBuf;

fn file() -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")?;
    Some(PathBuf::from(base).join("blue-alert-sender").join("address.txt"))
}

/// 저장해 둔 주소. 없으면 `None`.
pub fn load_address() -> Option<String> {
    let text = std::fs::read_to_string(file()?).ok()?;
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// 주소를 저장한다. 실패해도 프로그램은 그대로 쓸 수 있으므로 조용히 넘어간다.
pub fn save_address(address: &str) {
    let Some(path) = file() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, address.trim());
}
