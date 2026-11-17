# blue-alert-sender

블루 알림 수신기에 블루투스(BLE)로 명령을 보내는 Windows 11용 도구입니다.
Rust + egui/eframe로 만들었고, 실행 파일 하나로 UI와 CLI를 모두 씁니다.

- 그냥 실행하면 **UI**가 뜹니다.
- `-cli`를 붙이면 **명령줄**에서 명령 하나를 보냅니다.
- 페어링은 하지 않습니다. 수신기의 블루투스 주소로 바로 연결합니다.
- 설치가 필요 없습니다. `blue-alert-sender.exe` 하나만 복사하면 됩니다(Windows 기본 DLL만 씁니다).

## 블루 알림 수신기

"블루 알림"은 PC의 메신저 알림을 책상 위의 빨간 LED로 알려 주려고 직접 만든 장치입니다.
수신기는 ESP32 보드에 빨간 LED, 작은 OLED 화면, 확인 버튼을 단 것이고, BLE 이름은 `BlueAlert-01`입니다.
이 도구는 그 수신기를 시험하고 손으로 움직여 보는 데 씁니다.
수신기의 펌웨어와 회로는 이 저장소에 없습니다. 주고받는 규칙은 아래 [통신 규격](#통신-규격)에 있습니다.

## 받기

[릴리스](https://github.com/sukmin/blue-alert-sender/releases)에서 `blue-alert-sender-<버전>-win64.zip`을 받아 풉니다.
서명되지 않은 실행 파일이라 Windows나 회사 정책(SmartScreen, AppLocker 등)이 실행을 막을 수 있습니다.

## UI 모드

```
blue-alert-sender
blue-alert-sender -addr AA:BB:CC:DD:EE:FF
```

- **수신기 주소** 입력란에 수신기의 블루투스 주소를 적습니다. 마지막으로 연결에 성공한 주소를 기억합니다.
- **연결 / 끊기** 버튼. 명령 버튼을 누르면 연결되어 있지 않아도 먼저 연결합니다.
- 명령 버튼:

  | 묶음 | 버튼 | 보내는 명령 |
  |---|---|---|
  | 시험 | 연결 확인 | `@PING` |
  | 시험 | 시험 깜빡임 | `@TEST` |
  | 알림 | 느린 알림 | `@ALERT,SLOW,0` |
  | 알림 | 빠른 알림 | `@ALERT,FAST,0` |
  | 알림 | 계속 켜짐 | `@ALERT,ON,0` |
  | 알림 | 빠른 알림 10초 | `@ALERT,FAST,10` |
  | 알림 | 알림 끄기 | `@OFF` |
  | 밝기 | 밝기 30% / 60% / 100% | `@BRIGHT,30` / `60` / `100` |
  | 쉬기 | 쉬기 | `@SLEEP` |

- **기록**에 보낸 명령(→)과 받은 응답(←)이 나옵니다. 수신기의 확인 버튼을 누르면 `ACK`가 나옵니다.
- 연결되어 있는 동안 30초마다 `@PING`을 자동으로 보냅니다. 수신기는 90초 동안 아무것도 못 받으면
  알림을 가리고 "연결 없음"을 표시하므로, 알림이 계속 보이려면 이것이 필요합니다.
- 켜 둔 알림은 연결이 끊겨도 수신기에 남습니다. 끝내기 전에 **알림 끄기**를 누릅니다.

## CLI 모드

```
blue-alert-sender -cli -addr <주소> <명령>
blue-alert-sender -cli -addr <주소> -hold <초> <명령>
blue-alert-sender -cli scan [초]
blue-alert-sender -cli help
blue-alert-sender -version
```

PowerShell에서는 현재 폴더의 실행 파일을 `.\blue-alert-sender`처럼 `.\`를 붙여 부릅니다.

| 명령 | 뜻 |
|---|---|
| `ping` | 살아 있는지 확인 (`@PING` → `PONG`) |
| `test` | LED 3번 깜빡 (`@TEST`) |
| `alert [패턴] [초]` | 알림 켜기. 패턴은 `slow`(기본)·`fast`·`on`, 초는 0~3600(0이나 생략은 끌 때까지). `alert 10`처럼 초만 적으면 느린 알림 |
| `slow` / `fast` / `on` `[초]` | 위와 같음 |
| `off` | 알림 모두 끄기 (`@OFF`) |
| `sleep` | 쉬기 (`@SLEEP`) |
| `bright <1~100>` | 밝기 (`@BRIGHT`) |
| `raw <명령>` | 수신기 명령을 직접 적어 보냄. `@`는 빼고 적음 (예: `raw ALERT,ON,600`) |
| `@명령` | 위와 같음. PowerShell에서는 `@`와 쉼표가 다른 뜻이라 작은따옴표로 쌈 (예: `'@ALERT,ON,600'`) |

옵션:

- `-addr <주소>`: 수신기의 블루투스 주소. 빼면 저장해 둔 주소(UI나 CLI에서 마지막으로 연결에 성공한 주소)를 씁니다.
  저장된 주소가 없으면 종료 코드 2로 끝납니다.
- `-hold <초>`: 명령을 보낸 뒤 그 시간만큼 연결을 유지하며 지켜봅니다. 30초마다 `@PING`을 보내고,
  확인 버튼을 누르면 `ACK`를 보여 줍니다. **알림이 실제로 깜빡이는 것을 보려면 이 옵션을 씁니다.**
  명령만 보내고 끊으면 수신기는 곧바로 "연결 없음" 표시로 바뀌기 때문입니다.
- `scan [초]`: 주변의 BLE 기기를 찾아 주소와 신호 세기를 보여 줍니다. 수신기 주소를 모를 때 씁니다.

예:

```
blue-alert-sender -cli -addr AA:BB:CC:DD:EE:FF ping
blue-alert-sender -cli -addr AA:BB:CC:DD:EE:FF -hold 60 alert fast
blue-alert-sender -cli -addr AA:BB:CC:DD:EE:FF bright 30
blue-alert-sender -cli -addr AA:BB:CC:DD:EE:FF raw ALERT,ON,600
```

`ping`이 잘 됐을 때:

```
수신기: AA:BB:CC:DD:EE:FF
장치: BlueAlert-01
보냄: @PING
응답: PONG  (수신기가 살아 있습니다)
```

종료 코드: `0` 성공, `1` 실패(연결하지 못함, 수신기가 `ERR`로 답함, `-hold` 중 끊김, `scan`이 아무것도 못 찾음), `2` 사용법이 틀림.

스크립트에서는 출력 글 대신 종료 코드로 판단합니다. 출력을 파일이나 변수로 받으면 UTF-8로 나오는데,
Windows PowerShell 5.1은 이것을 다른 글자표로 읽어 한글이 깨질 수 있습니다
(먼저 `[Console]::OutputEncoding = [Text.Encoding]::UTF8`).

## 통신 규격

수신기는 BLE의 Nordic UART Service(NUS)로 글자를 주고받습니다. 페어링과 암호화는 쓰지 않습니다.

| 항목 | 값 |
|---|---|
| 서비스 | `6E400001-B5A3-F393-E0A9-E50E24DCCA9E` |
| RX 특성 (PC → 수신기, 쓰기) | `6E400002-B5A3-F393-E0A9-E50E24DCCA9E` |
| TX 특성 (수신기 → PC, 알림·읽기) | `6E400003-B5A3-F393-E0A9-E50E24DCCA9E` |
| 글자 | ASCII. 한 번 쓰기에 명령 하나, 최대 40글자. 응답 끝에는 줄바꿈 하나가 붙음 |
| 연결 수 | 한 번에 하나 |

| 명령 | 뜻 | 응답 |
|---|---|---|
| `@PING` | 살아 있는지 확인 | `PONG` |
| `@TEST` | LED를 3번 깜빡임 | `OK` |
| `@ALERT,패턴,시간` | 알림 켜기. 패턴은 `ON`(계속 켜짐)·`SLOW`(1초에 1번)·`FAST`(1초에 2.5번), 시간은 0~3600초(0은 끌 때까지) | `OK` |
| `@OFF` | 알림 모두 끄기 | `OK` |
| `@SLEEP` | 쉬기. 알림을 지우고 LED를 끔. `@ALERT`나 새 연결로 풀림 | `OK` |
| `@BRIGHT,숫자` | LED 밝기 1~100% | `OK` |

그 밖의 응답: `ACK`(수신기의 확인 버튼으로 알림을 껐음. 언제든 올 수 있음), `READY,3.1`(연결 직후의 인사),
`ERR,FORMAT`·`ERR,PATTERN`·`ERR,TIME`·`ERR,BRIGHT`(명령을 받아들이지 않음).

수신기는 90초 동안 아무 명령도 받지 못하면 연결이 살아 있지 않다고 보고 3초마다 짧게 반짝입니다(10분 뒤 멈춤).
그래서 연결을 잡고 있는 프로그램은 30초마다 `@PING`을 보냅니다.

## 빌드

Rust(stable, `x86_64-pc-windows-msvc`)가 필요합니다.

```
cargo build --release
```

결과는 `target\release\blue-alert-sender.exe`입니다. 배포용 묶음은 `scripts\package.bat`으로 만듭니다
(`dist\blue-alert-sender-<버전>-win64.zip`).

블루투스를 실제로 쓰는 시험은 평소에는 건너뜁니다.

```
cargo test                                   # 명령·주소·인자 해석 (13개)
cargo test reports_failure -- --ignored      # 수신기 없는 주소로 연결 실패 처리 (15초쯤)
set BLUE_ALERT_SENDER_ADDR=AA:BB:CC:DD:EE:FF
cargo test live_receiver -- --ignored        # 켜져 있는 수신기가 필요함
```

## 구조

| 파일 | 하는 일 |
|---|---|
| `src/main.rs` | 인자를 보고 UI나 CLI로 나눔 |
| `src/cli.rs` | 인자 해석, CLI 모드 |
| `src/ui.rs` | UI 모드(egui) |
| `src/worker.rs` | UI 뒤에서 블루투스를 맡는 스레드, 30초마다 `@PING` |
| `src/ble.rs` | 블루투스 연결: 주소로 열기, 처음이면 검색, 명령 쓰기, 응답 받기 |
| `src/protocol.rs` | 주소·명령·응답의 글자 규칙 |
| `src/settings.rs` | 마지막 주소 기억 (`%LOCALAPPDATA%\blue-alert-sender\address.txt`) |
| `src/win.rs` | 콘솔 창 놓기, 시각 |
| `assets/blue-alert-sender.manifest` | DPI 설정, 탐색기에서 실행할 때 콘솔 창을 만들지 않게 하는 설정 |
| `scripts/package.bat` | 배포용 폴더와 zip 만들기 |

## 확인한 범위

0.1.0 기준, Windows 11(빌드 26200)과 USB 블루투스 동글에서 확인한 것입니다.

| 내용 | 확인 |
|---|---|
| UI의 버튼 11개 전부(연결 확인, 시험 깜빡임, 알림 4가지, 알림 끄기, 밝기 3가지, 쉬기) | 실제 수신기로 확인. 모두 `OK`·`PONG`이 왔고 LED가 그대로 동작함 |
| 수신기의 확인 버튼을 눌렀을 때의 `ACK`, 30초마다의 자동 `@PING` | 실제 수신기로 확인 |
| CLI의 `ping`과 `-hold` | 실제 수신기로 확인 |
| CLI의 나머지 명령(`test`, `alert`, `off`, `sleep`, `bright`, `raw`) | 명령을 글자로 바꾸는 부분은 자동 시험으로 확인. 보내는 길은 `ping`과 같음. 수신기로 하나씩 보내 보지는 않음 |
| 수신기가 꺼져 있을 때, 없는 주소일 때의 실패 | 실제로 실행해 확인 |
| UI 창이 연결해 있는 동안 같은 PC의 CLI가 거절되는 것 | 실제로 실행해 확인(Windows가 AccessDenied를 돌려줌). 그 뒤 안내 문구를 고쳤고, 고친 문구가 나오는 것은 보지 못함 |
| 주변 기기 찾기(`scan`) | 실제로 실행해 확인. 수신기가 목록에 나오는 것은 보지 못함 |
| 처음 만나는 PC에서 검색한 뒤 연결되는 것 | 확인하지 못함 |
| Windows 11 24H2보다 옛 Windows, 어두운 모드에서의 창 모습 | 확인하지 못함 |

## 알아 둘 것

- **콘솔 창**: 콘솔 프로그램으로 빌드했습니다. CLI 모드에서 셸이 끝날 때까지 기다리고 종료 코드를 받게
  하려는 것입니다. UI 모드로 탐색기에서 실행하면 Windows 11 24H2부터는 콘솔 창이 생기지 않고,
  그보다 옛 Windows에서는 콘솔 창이 잠깐 보였다 사라집니다.
- **처음 만나는 PC**: Windows는 최근에 신호를 본 기기만 주소로 열 수 있습니다. 처음이면 스스로
  주변을 검색(최대 15초)한 뒤 연결합니다.
- **한 PC에서 한 프로그램만**: UI 창이 연결해 있는 동안에는 같은 PC의 CLI가 연결하지 못합니다
  ("이 PC의 다른 프로그램이 수신기를 쓰고 있습니다").
- **응답 받는 방식**: 알림(Notify)을 켜서 바로 받고, 켜지 못하면 1초마다 값을 읽습니다. 알림이 1.5초 안에 오지 않으면
  수신기에 남은 마지막 응답을 읽어 보여 주므로, 드물게 앞 명령의 응답이 나올 수 있습니다.
- **끊김**: UI와 `-hold`는 연결이 끊기면 3초쯤 안에 알아챕니다.
- **기억하는 것**: 마지막으로 연결에 성공한 주소 하나뿐입니다(`%LOCALAPPDATA%\blue-alert-sender\address.txt`).
  도구를 없앨 때는 실행 파일과 이 폴더를 지웁니다.
