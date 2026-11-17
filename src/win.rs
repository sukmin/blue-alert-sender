//! Windows에만 있는 작은 기능: 콘솔 창 놓기, 지금 시각.

use windows::Win32::System::Console::{FreeConsole, GetConsoleProcessList, GetConsoleWindow};
use windows::Win32::System::SystemInformation::GetLocalTime;

/// UI 모드에서, 이 프로그램만을 위해 새로 만들어진 콘솔 창이 있으면 닫는다.
///
/// 이 프로그램은 콘솔 프로그램으로 빌드한다(CLI 모드에서 셸이 끝날 때까지 기다리게 하려고).
/// Windows 11 24H2부터는 매니페스트의 `consoleAllocationPolicy`가 탐색기에서 실행할 때
/// 콘솔 창을 만들지 않게 해 주지만, 그보다 옛 Windows에서는 창이 생기므로 여기서 놓아 준다.
/// 터미널에서 실행했다면(콘솔을 셸과 함께 쓰는 중이면) 건드리지 않는다.
pub fn release_own_console() {
    unsafe {
        if GetConsoleWindow().0.is_null() {
            return;
        }
        let mut ids = [0u32; 2];
        if GetConsoleProcessList(&mut ids) <= 1 {
            let _ = FreeConsole();
        }
    }
}

/// 지금 시각을 `09:10:02` 모양으로 적는다.
pub fn clock() -> String {
    let now = unsafe { GetLocalTime() };
    format!("{:02}:{:02}:{:02}", now.wHour, now.wMinute, now.wSecond)
}
