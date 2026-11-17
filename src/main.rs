//! 블루 알림 수신기에 명령을 보내는 도구 (Windows 11).
//!
//! 그냥 실행하면 UI가 뜨고, `-cli`를 붙이면 명령줄에서 명령 하나를 보낸다.
//!
//!   blue-alert-sender                              UI
//!   blue-alert-sender -cli -addr <주소> <명령>     CLI (자세한 것은 `blue-alert-sender -cli help`)
//!
//! 콘솔 프로그램으로 빌드한다. 그래야 CLI 모드에서 셸이 끝날 때까지 기다리고, 출력과 종료 코드를
//! 그대로 받는다. UI 모드에서 콘솔 창이 뜨지 않게 하는 것은 매니페스트의 `consoleAllocationPolicy`와
//! `win::release_own_console`이 맡는다.

mod ble;
mod cli;
mod protocol;
mod settings;
mod ui;
mod win;
mod worker;

use std::process::ExitCode;

pub const APP_NAME: &str = "blue-alert-sender";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match cli::parse_args(&args) {
        cli::Mode::Help => {
            cli::print_usage();
            ExitCode::SUCCESS
        }
        cli::Mode::Version => {
            println!("blue-alert-sender {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        cli::Mode::Bad(reason) => {
            eprintln!("{reason}");
            eprintln!("사용법은 blue-alert-sender -cli help 로 볼 수 있습니다.");
            ExitCode::from(2)
        }
        cli::Mode::Send { address, hold, words } => cli::send(address, hold, &words),
        cli::Mode::Scan { seconds } => cli::scan(seconds),
        cli::Mode::Ui { address } => {
            win::release_own_console();
            match ui::run(address) {
                Ok(()) => ExitCode::SUCCESS,
                Err(err) => {
                    eprintln!("창을 열지 못했습니다: {err}");
                    ExitCode::from(1)
                }
            }
        }
    }
}
