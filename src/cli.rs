//! 명령줄 인자 읽기와 CLI 모드.

use std::process::ExitCode;
use std::time::{Duration, Instant};

use crate::ble::{self, Link};
use crate::protocol;
use crate::settings;
use crate::win::clock;

/// 연결을 유지할 때 `@PING`을 보내는 간격. 수신기는 90초 동안 아무것도 못 받으면 끊긴 것으로 본다.
const PING_INTERVAL: Duration = Duration::from_secs(30);
const DEFAULT_SCAN_SECONDS: u64 = 8;
/// Windows가 "끊김"이라고 한 뒤 이만큼 이어지면 연결이 끊긴 것으로 본다.
const LOST_AFTER: Duration = Duration::from_secs(3);

pub const USAGE: &str = "\
blue-alert-sender: 블루 알림 수신기에 블루투스로 명령을 보내는 도구

사용법:
  blue-alert-sender                              UI로 실행 (기본)
  blue-alert-sender -addr <주소>                 그 주소를 채워 UI로 실행
  blue-alert-sender -cli -addr <주소> <명령>     명령 하나를 보내고 응답을 출력
  blue-alert-sender -cli scan [초]               주변의 BLE 기기를 찾아 주소를 보여 줌 (기본 8초)
  blue-alert-sender -cli help                    이 도움말
  blue-alert-sender -version                     버전

옵션 (CLI):
  -addr <주소>   수신기의 블루투스 주소. 예: AA:BB:CC:DD:EE:FF
                 빼면 저장해 둔 주소(마지막으로 연결에 성공한 주소)를 씁니다.
  -hold <초>     명령을 보낸 뒤 그 시간만큼 연결을 유지하며 지켜봅니다.
                 30초마다 @PING을 보내고, 확인 버튼을 누르면 ACK를 보여 줍니다.
                 알림이 실제로 깜빡이는 것을 보려면 이 옵션이 필요합니다.
                 (명령만 보내고 끊으면 수신기는 \"연결 없음\" 표시로 바뀝니다.)

명령:
";

pub const EXAMPLES: &str = "
예:
  blue-alert-sender -cli -addr AA:BB:CC:DD:EE:FF ping
  blue-alert-sender -cli -addr AA:BB:CC:DD:EE:FF test
  blue-alert-sender -cli -addr AA:BB:CC:DD:EE:FF -hold 60 alert fast
  blue-alert-sender -cli -addr AA:BB:CC:DD:EE:FF bright 30
  blue-alert-sender -cli -addr AA:BB:CC:DD:EE:FF on 600
  blue-alert-sender -cli -addr AA:BB:CC:DD:EE:FF raw ALERT,ON,600

종료 코드: 0 성공, 1 실패(연결 못 함, 수신기가 ERR로 답함), 2 사용법이 틀림";

pub fn print_usage() {
    println!("{USAGE}{}\n{EXAMPLES}", protocol::COMMAND_HELP);
}

/// 명령줄에서 읽은 실행 방식.
#[derive(Debug, PartialEq)]
pub enum Mode {
    /// UI로 실행. 주소를 미리 채울 수 있다.
    Ui { address: Option<String> },
    /// 명령 하나를 보낸다.
    Send { address: Option<String>, hold: Option<u64>, words: Vec<String> },
    /// 주변 기기를 찾는다.
    Scan { seconds: u64 },
    Help,
    Version,
    /// 인자가 틀렸다. 이유를 담는다.
    Bad(String),
}

pub fn parse_args(args: &[String]) -> Mode {
    let mut cli = false;
    let mut help = false;
    let mut version = false;
    let mut address = None;
    let mut hold = None;
    let mut words: Vec<String> = Vec::new();

    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let lower = arg.to_lowercase();
        match lower.as_str() {
            "-cli" | "--cli" | "/cli" => cli = true,
            "-h" | "--help" | "-?" | "/?" => help = true,
            "-version" | "--version" | "-v" => version = true,
            "-addr" | "--addr" | "-a" => match it.next() {
                Some(value) => address = Some(value.clone()),
                None => return Mode::Bad("-addr 뒤에 주소가 없습니다".to_owned()),
            },
            "-hold" | "--hold" => match it.next().map(|v| v.parse::<u64>()) {
                Some(Ok(seconds)) => hold = Some(seconds),
                _ => return Mode::Bad("-hold 뒤에는 초(숫자)를 적습니다".to_owned()),
            },
            _ if lower.starts_with("-addr=") || lower.starts_with("--addr=") => {
                address = Some(arg[arg.find('=').map_or(0, |i| i + 1)..].to_owned());
            }
            _ if arg.starts_with('-') && words.is_empty() => {
                return Mode::Bad(format!("모르는 옵션입니다: {arg}"));
            }
            _ => words.push(arg.clone()),
        }
    }

    if help || (cli && words.first().is_some_and(|w| w.eq_ignore_ascii_case("help"))) {
        return Mode::Help;
    }
    if version || (cli && words.first().is_some_and(|w| w.eq_ignore_ascii_case("version"))) {
        return Mode::Version;
    }
    if !cli {
        if !words.is_empty() {
            return Mode::Bad(format!("명령을 보내려면 -cli를 붙입니다: {}", words.join(" ")));
        }
        if hold.is_some() {
            return Mode::Bad("-hold는 -cli와 함께 씁니다".to_owned());
        }
        return Mode::Ui { address };
    }
    if words.first().is_some_and(|w| w.eq_ignore_ascii_case("scan")) {
        return match words.get(1).map(|v| v.parse::<u64>()) {
            None => Mode::Scan { seconds: DEFAULT_SCAN_SECONDS },
            Some(Ok(seconds)) if (1..=120).contains(&seconds) && words.len() == 2 => Mode::Scan { seconds },
            _ => Mode::Bad("scan 뒤에는 초(1~120)만 적습니다".to_owned()),
        };
    }
    if words.is_empty() {
        return Mode::Bad("보낼 명령이 없습니다".to_owned());
    }
    Mode::Send { address, hold, words }
}

/// CLI 모드에서 명령 하나를 보낸다.
pub fn send(address: Option<String>, hold: Option<u64>, words: &[String]) -> ExitCode {
    let command = match protocol::parse_command(words) {
        Ok(command) => command,
        Err(reason) => return usage_error(&reason),
    };
    let Some(address_text) = address.or_else(settings::load_address) else {
        return usage_error("수신기 주소가 필요합니다. -addr <주소>를 붙이세요");
    };
    let address = match protocol::parse_address(&address_text) {
        Ok(address) => address,
        Err(reason) => return usage_error(&reason),
    };

    ble::init_thread();
    println!("수신기: {}", protocol::format_address(address));
    let mut link = match Link::connect(address, &mut |message| println!("{message}")) {
        Ok(link) => link,
        Err(err) => {
            eprintln!("{err}");
            return ExitCode::from(1);
        }
    };
    settings::save_address(&protocol::format_address(address));
    println!("장치: {}", if link.name().is_empty() { "(이름을 받지 못함)" } else { link.name() });

    let reply = match link.send(&command) {
        Ok(reply) => reply,
        Err(err) => {
            eprintln!("보냄: {command}");
            eprintln!("{err}");
            return ExitCode::from(1);
        }
    };
    println!("보냄: {command}");
    print_reply("응답", &reply);
    let ok = protocol::is_success(&reply);

    if let Some(seconds) = hold {
        if let Err(err) = watch(&mut link, Duration::from_secs(seconds)) {
            eprintln!("{}  {err}", clock());
            return ExitCode::from(1);
        }
    }
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

/// 연결을 유지하며 `duration` 동안 지켜본다. 끊기면 오류를 돌려준다.
fn watch(link: &mut Link, duration: Duration) -> ble::Result<()> {
    println!(
        "{}  {}초 동안 연결을 유지합니다. 30초마다 @PING을 보냅니다. (끝내려면 Ctrl+C)",
        clock(),
        duration.as_secs()
    );
    if !link.uses_notify() {
        println!("{}  응답을 1초마다 읽는 방식으로 지켜봅니다.", clock());
    }
    let end = Instant::now() + duration;
    let mut next_ping = Instant::now() + PING_INTERVAL;
    let (mut pings, mut pongs) = (0u32, 0u32);
    let mut lost_since: Option<Instant> = None;
    while Instant::now() < end {
        std::thread::sleep(Duration::from_millis(if link.uses_notify() { 200 } else { 1000 }));
        // 수신기가 꺼지거나 멀어지면 다음 @PING을 기다리지 않고 알아챈다
        if link.is_connected() {
            lost_since = None;
        } else {
            let since = *lost_since.get_or_insert_with(Instant::now);
            if since.elapsed() >= LOST_AFTER {
                return Err(ble::BleError::Lost);
            }
            continue;
        }
        for event in link.take_events()? {
            print_reply(&format!("{}  수신", clock()), &event);
        }
        if Instant::now() >= next_ping {
            pings += 1;
            let reply = link.send("@PING")?;
            if reply == "PONG" {
                pongs += 1;
            } else {
                print_reply(&format!("{}  @PING의 응답이 PONG이 아님", clock()), &reply);
            }
            next_ping = Instant::now() + PING_INTERVAL;
        }
    }
    println!("{}  끝: @PING {pings}번 중 PONG {pongs}번", clock());
    Ok(())
}

fn print_reply(label: &str, reply: &str) {
    let meaning = protocol::describe_reply(reply);
    if reply.is_empty() {
        println!("{label}: (없음)");
    } else if meaning.is_empty() {
        println!("{label}: {reply}");
    } else {
        println!("{label}: {reply}  ({meaning})");
    }
}

/// 주변의 BLE 기기를 찾아 보여 준다.
pub fn scan(seconds: u64) -> ExitCode {
    ble::init_thread();
    println!("주변의 BLE 기기를 {seconds}초 동안 찾습니다...");
    let mut found = match ble::scan(Duration::from_secs(seconds), None) {
        Ok(found) => found,
        Err(err) => {
            eprintln!("{err}");
            return ExitCode::from(1);
        }
    };
    if found.is_empty() {
        println!("찾은 기기가 없습니다. PC의 Bluetooth가 켜져 있는지 확인하세요.");
        return ExitCode::from(1);
    }
    // 수신기일 가능성이 높은 것을 위로, 그다음은 신호가 센 순서로
    found.sort_by_key(|f| (!is_receiver(f), -(f.rssi as i32)));
    // 한글은 화면에서 두 칸을 차지하므로 머리글의 빈칸은 손으로 맞춘다
    println!("주소                  세기  이름");
    for f in &found {
        let mark = if is_receiver(f) { "  ← 수신기로 보임" } else { "" };
        println!("{}  {:>4}dBm  {}{}", protocol::format_address(f.address), f.rssi, f.name, mark);
    }
    println!("기기 {}개. 세기는 0에 가까울수록 가깝습니다.", found.len());
    ExitCode::SUCCESS
}

fn is_receiver(found: &ble::Found) -> bool {
    found.has_nus || found.name.starts_with("BlueAlert")
}

fn usage_error(reason: &str) -> ExitCode {
    eprintln!("{reason}");
    eprintln!("사용법은 blue-alert-sender -cli help 로 볼 수 있습니다.");
    ExitCode::from(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Mode {
        parse_args(&args.iter().map(|a| (*a).to_owned()).collect::<Vec<_>>())
    }

    fn words(items: &[&str]) -> Vec<String> {
        items.iter().map(|w| (*w).to_owned()).collect()
    }

    #[test]
    fn runs_ui_by_default() {
        assert_eq!(parse(&[]), Mode::Ui { address: None });
        assert_eq!(
            parse(&["-addr", "AA:BB:CC:DD:EE:FF"]),
            Mode::Ui { address: Some("AA:BB:CC:DD:EE:FF".to_owned()) }
        );
    }

    #[test]
    fn parses_cli_commands() {
        assert_eq!(
            parse(&["-cli", "-addr", "AA:BB:CC:DD:EE:FF", "ping"]),
            Mode::Send { address: Some("AA:BB:CC:DD:EE:FF".to_owned()), hold: None, words: words(&["ping"]) }
        );
        // 옵션과 명령의 순서는 바뀌어도 된다
        assert_eq!(
            parse(&["-addr=AA:BB:CC:DD:EE:FF", "alert", "fast", "10", "-cli", "-hold", "60"]),
            Mode::Send {
                address: Some("AA:BB:CC:DD:EE:FF".to_owned()),
                hold: Some(60),
                words: words(&["alert", "fast", "10"]),
            }
        );
        assert_eq!(
            parse(&["-cli", "@ALERT,FAST,10"]),
            Mode::Send { address: None, hold: None, words: words(&["@ALERT,FAST,10"]) }
        );
    }

    #[test]
    fn parses_scan_and_help() {
        assert_eq!(parse(&["-cli", "scan"]), Mode::Scan { seconds: DEFAULT_SCAN_SECONDS });
        assert_eq!(parse(&["-cli", "scan", "20"]), Mode::Scan { seconds: 20 });
        assert_eq!(parse(&["-cli", "help"]), Mode::Help);
        assert_eq!(parse(&["--help"]), Mode::Help);
        assert_eq!(parse(&["-h"]), Mode::Help);
        assert_eq!(parse(&["-version"]), Mode::Version);
        assert_eq!(parse(&["-cli", "version"]), Mode::Version);
    }

    #[test]
    fn rejects_bad_arguments() {
        assert!(matches!(parse(&["-cli"]), Mode::Bad(_)));
        assert!(matches!(parse(&["-cli", "-addr"]), Mode::Bad(_)));
        assert!(matches!(parse(&["-cli", "-hold", "abc", "ping"]), Mode::Bad(_)));
        assert!(matches!(parse(&["-cli", "scan", "0"]), Mode::Bad(_)));
        assert!(matches!(parse(&["-cli", "-what", "ping"]), Mode::Bad(_)));
        // -cli 없이 명령을 적으면 UI가 뜨지 않고 알려 준다
        assert!(matches!(parse(&["ping"]), Mode::Bad(_)));
        assert!(matches!(parse(&["-hold", "10"]), Mode::Bad(_)));
    }
}
