//! 수신기와 주고받는 글자: 주소, 명령, 응답.
//!
//! 규격은 README의 "통신 규격"에 있다. 명령은 ASCII, 한 번에 하나, 최대 40글자.

/// 한 번에 보낼 수 있는 명령의 최대 길이(글자).
pub const MAX_COMMAND_LEN: usize = 40;

/// `AA:BB:CC:DD:EE:FF`, `AA-BB-…`, `AABBCCDDEEFF` 모양의 주소를 숫자로 바꾼다.
pub fn parse_address(text: &str) -> Result<u64, String> {
    let hex: String = text.chars().filter(|c| !matches!(c, ':' | '-' | ' ')).collect();
    if hex.len() != 12 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!(
            "주소가 올바르지 않습니다: \"{}\" (예: AA:BB:CC:DD:EE:FF)",
            text.trim()
        ));
    }
    u64::from_str_radix(&hex, 16).map_err(|_| "주소를 읽지 못했습니다".to_owned())
}

/// 숫자 주소를 `AA:BB:CC:DD:EE:FF` 모양으로 적는다.
pub fn format_address(address: u64) -> String {
    let b = address.to_be_bytes();
    format!("{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}", b[2], b[3], b[4], b[5], b[6], b[7])
}

/// CLI에 적은 낱말들을 수신기 명령 한 줄로 바꾼다.
///
/// `ping`, `test`, `off`, `sleep`, `bright 30`, `alert fast 10`, `fast 10` 같은 쉬운 말과
/// `@ALERT,FAST,10` 같은 원래 명령을 모두 받는다.
pub fn parse_command(words: &[String]) -> Result<String, String> {
    let Some(first) = words.first() else {
        return Err("명령이 없습니다".to_owned());
    };
    if first.starts_with('@') {
        // 원래 명령: 낱말 사이에 띄어 쓴 것은 붙인다 (@ALERT, FAST, 10 → @ALERT,FAST,10)
        let raw: String = words.concat().to_uppercase();
        return check_raw(&raw).map(|()| raw);
    }
    let lower: Vec<String> = words.iter().map(|w| w.to_lowercase()).collect();
    let rest = &lower[1..];
    match lower[0].as_str() {
        // `@` 없이 원래 명령을 적는 방법. PowerShell은 `@`와 쉼표를 제 문법으로 읽어 버리므로 필요하다.
        // PowerShell이 `ALERT,ON,600`을 세 낱말로 쪼개 넘겨도 쉼표로 다시 잇는다.
        "raw" => {
            if rest.is_empty() {
                return Err("raw 뒤에 수신기 명령을 적습니다 (예: raw ALERT,ON,600)".to_owned());
            }
            let body = rest.join(",").to_uppercase();
            let raw = format!("@{}", body.trim_start_matches('@'));
            check_raw(&raw).map(|()| raw)
        }
        "ping" => no_more(rest, "@PING"),
        "test" => no_more(rest, "@TEST"),
        "off" => no_more(rest, "@OFF"),
        "sleep" => no_more(rest, "@SLEEP"),
        "bright" => {
            let [value] = rest else {
                return Err("밝기는 숫자 하나를 줍니다 (예: bright 30)".to_owned());
            };
            let n = parse_number(value, 1, 100, "밝기는")?;
            Ok(format!("@BRIGHT,{n}"))
        }
        "alert" => match rest {
            [] => Ok("@ALERT,SLOW,0".to_owned()),
            // 패턴 없이 초만 적으면 느린 알림 (alert 10)
            [seconds] if seconds.chars().all(|c| c.is_ascii_digit()) => alert("slow", rest),
            [pattern, more @ ..] => alert(pattern, more),
        },
        "slow" | "fast" | "on" => alert(&lower[0], rest),
        other => Err(format!("모르는 명령입니다: {other}")),
    }
}

fn no_more(rest: &[String], command: &str) -> Result<String, String> {
    if rest.is_empty() {
        Ok(command.to_owned())
    } else {
        Err(format!("{command} 뒤에는 아무것도 붙지 않습니다"))
    }
}

fn alert(pattern: &str, rest: &[String]) -> Result<String, String> {
    let pattern = match pattern {
        "slow" => "SLOW",
        "fast" => "FAST",
        "on" => "ON",
        other => return Err(format!("패턴은 slow, fast, on 가운데 하나입니다: {other}")),
    };
    let seconds = match rest {
        [] => 0,
        [value] => parse_number(value, 0, 3600, "시간(초)은")?,
        _ => return Err("알림은 패턴과 시간(초)까지만 줍니다 (예: alert fast 10)".to_owned()),
    };
    Ok(format!("@ALERT,{pattern},{seconds}"))
}

fn parse_number(text: &str, min: u32, max: u32, subject: &str) -> Result<u32, String> {
    match text.parse::<u32>() {
        Ok(n) if (min..=max).contains(&n) => Ok(n),
        _ => Err(format!("{subject} {min}~{max} 사이의 숫자입니다: {text}")),
    }
}

/// 원래 명령을 그대로 보낼 때의 기본 검사(길이, 글자). 뜻이 맞는지는 수신기가 판단한다.
pub fn check_raw(command: &str) -> Result<(), String> {
    if command.len() > MAX_COMMAND_LEN {
        return Err(format!("명령은 {MAX_COMMAND_LEN}글자까지입니다"));
    }
    if !command.chars().all(|c| c.is_ascii_graphic()) {
        return Err("명령에는 빈칸 없는 영문·숫자·기호만 씁니다".to_owned());
    }
    Ok(())
}

/// 수신기의 응답이 "잘 됐다"는 뜻인지.
pub fn is_success(reply: &str) -> bool {
    !reply.is_empty() && !reply.starts_with("ERR")
}

/// 명령과 상관없이 올 수 있는 응답(확인 버튼, 연결 인사)인지.
pub fn is_unsolicited(reply: &str) -> bool {
    reply == "ACK" || reply.starts_with("READY")
}

/// 응답을 사람이 읽을 말로 풀어 준다.
pub fn describe_reply(reply: &str) -> &'static str {
    match reply {
        "OK" => "명령을 처리했습니다",
        "PONG" => "수신기가 살아 있습니다",
        "ACK" => "확인 버튼으로 알림을 껐습니다",
        "ERR,FORMAT" => "수신기가 모르는 명령입니다",
        "ERR,PATTERN" => "알림의 패턴이 ON, SLOW, FAST가 아닙니다",
        "ERR,TIME" => "알림의 시간이 0~3600초가 아닙니다",
        "ERR,BRIGHT" => "밝기가 1~100이 아닙니다",
        r if r.starts_with("READY") => "연결 직후의 인사입니다",
        _ => "",
    }
}

/// CLI 도움말에 싣는 명령 목록.
pub const COMMAND_HELP: &str = "  ping                  살아 있는지 확인 (@PING → PONG)
  test                  LED 3번 깜빡 (@TEST)
  alert [패턴] [초]     알림 켜기. 패턴은 slow(기본)·fast·on, 초는 0~3600(0이나 생략은 끌 때까지)
  slow|fast|on [초]     위와 같음 (alert를 줄여 쓴 것)
  off                   알림 모두 끄기 (@OFF)
  sleep                 쉬기 (@SLEEP)
  bright <1~100>        밝기 (@BRIGHT)
  raw <명령>            수신기 명령을 그대로 보냄. @는 빼고 적음 (예: raw ALERT,ON,600)
  @명령                 위와 같음. PowerShell에서는 작은따옴표로 쌈 (예: '@ALERT,ON,600')";

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd(words: &[&str]) -> Result<String, String> {
        parse_command(&words.iter().map(|w| (*w).to_owned()).collect::<Vec<_>>())
    }

    #[test]
    fn parses_addresses() {
        assert_eq!(parse_address("AA:BB:CC:DD:EE:FF"), Ok(0xAABB_CCDD_EEFF));
        assert_eq!(parse_address("aa-bb-cc-dd-ee-ff"), Ok(0xAABB_CCDD_EEFF));
        assert_eq!(parse_address(" AABBCCDDEEFF "), Ok(0xAABB_CCDD_EEFF));
        assert!(parse_address("AA:BB:CC:DD:EE").is_err());
        assert!(parse_address("AA:BB:CC:DD:EE:FG").is_err());
        assert!(parse_address("").is_err());
    }

    #[test]
    fn formats_addresses() {
        assert_eq!(format_address(0xAABB_CCDD_EEFF), "AA:BB:CC:DD:EE:FF");
        assert_eq!(format_address(0x0000_0000_0001), "00:00:00:00:00:01");
    }

    #[test]
    fn parses_friendly_commands() {
        assert_eq!(cmd(&["ping"]), Ok("@PING".to_owned()));
        assert_eq!(cmd(&["TEST"]), Ok("@TEST".to_owned()));
        assert_eq!(cmd(&["off"]), Ok("@OFF".to_owned()));
        assert_eq!(cmd(&["sleep"]), Ok("@SLEEP".to_owned()));
        assert_eq!(cmd(&["bright", "30"]), Ok("@BRIGHT,30".to_owned()));
        assert_eq!(cmd(&["alert"]), Ok("@ALERT,SLOW,0".to_owned()));
        assert_eq!(cmd(&["alert", "fast"]), Ok("@ALERT,FAST,0".to_owned()));
        assert_eq!(cmd(&["alert", "fast", "10"]), Ok("@ALERT,FAST,10".to_owned()));
        assert_eq!(cmd(&["on", "600"]), Ok("@ALERT,ON,600".to_owned()));
        assert_eq!(cmd(&["slow"]), Ok("@ALERT,SLOW,0".to_owned()));
    }

    #[test]
    fn rejects_bad_commands() {
        assert!(cmd(&[]).is_err());
        assert!(cmd(&["blink"]).is_err());
        assert!(cmd(&["ping", "now"]).is_err());
        assert!(cmd(&["bright"]).is_err());
        assert!(cmd(&["bright", "0"]).is_err());
        assert!(cmd(&["bright", "101"]).is_err());
        assert!(cmd(&["alert", "zzz"]).is_err());
        assert!(cmd(&["alert", "fast", "3601"]).is_err());
        assert!(cmd(&["alert", "fast", "10", "5"]).is_err());
    }

    #[test]
    fn accepts_raw_without_the_at_sign() {
        assert_eq!(cmd(&["raw", "ALERT,ON,600"]), Ok("@ALERT,ON,600".to_owned()));
        // PowerShell은 쉼표로 이은 것을 낱말 셋으로 쪼개 넘긴다
        assert_eq!(cmd(&["raw", "alert", "on", "600"]), Ok("@ALERT,ON,600".to_owned()));
        assert_eq!(cmd(&["raw", "@PING"]), Ok("@PING".to_owned()));
        assert_eq!(cmd(&["raw", "ALERT,ZZZ"]), Ok("@ALERT,ZZZ".to_owned()));
        assert!(cmd(&["raw"]).is_err());
    }

    #[test]
    fn takes_seconds_without_a_pattern() {
        assert_eq!(cmd(&["alert", "10"]), Ok("@ALERT,SLOW,10".to_owned()));
        assert_eq!(cmd(&["alert", "0"]), Ok("@ALERT,SLOW,0".to_owned()));
        assert!(cmd(&["alert", "3601"]).is_err());
    }

    #[test]
    fn passes_raw_commands() {
        assert_eq!(cmd(&["@alert,fast,10"]), Ok("@ALERT,FAST,10".to_owned()));
        assert_eq!(cmd(&["@ALERT,", "FAST,", "10"]), Ok("@ALERT,FAST,10".to_owned()));
        // 뜻이 틀린 원래 명령은 그대로 보내 수신기의 ERR을 본다
        assert_eq!(cmd(&["@ALERT,ZZZ"]), Ok("@ALERT,ZZZ".to_owned()));
        assert!(cmd(&["@ALERT,FAST,10,AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"]).is_err());
    }

    #[test]
    fn classifies_replies() {
        assert!(is_success("OK"));
        assert!(is_success("PONG"));
        assert!(!is_success("ERR,PATTERN"));
        assert!(!is_success(""));
        assert!(is_unsolicited("ACK"));
        assert!(is_unsolicited("READY,3.1"));
        assert!(!is_unsolicited("PONG"));
    }
}
