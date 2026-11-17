blue-alert-sender {VERSION} - 사용법
====================================

블루 알림 수신기에 블루투스(BLE)로 명령을 보내는 Windows 11용 도구입니다.

이 폴더의 구성
  blue-alert-sender.exe   도구 본체 (실행 파일 하나, 설치 필요 없음)
  README.md               자세한 설명 (명령, 통신 규격, 빌드 방법, 확인한 범위)

준비
  1. 수신기를 충전기에 꽂아 켭니다. LED가 3번 깜빡인 뒤 3초마다 짧게 반짝입니다.
  2. PC의 Bluetooth를 켭니다(설정 > Bluetooth 및 장치). 블루투스가 없는 PC는 USB 동글이 필요합니다.
  3. 수신기의 블루투스 주소를 준비합니다. 모르면 아래 "주소를 모를 때"를 봅니다.
  페어링은 하지 않습니다. Windows 설정에서 수신기를 추가하지 않습니다.

UI로 쓰기
  blue-alert-sender.exe 를 두 번 눌러 실행합니다.
  수신기 주소를 적고 버튼을 누릅니다. 연결되어 있지 않으면 먼저 연결합니다.
    시험   연결 확인, 시험 깜빡임
    알림   느린 알림, 빠른 알림, 계속 켜짐, 빠른 알림 10초, 알림 끄기
    밝기   30%, 60%, 100%
    쉬기   쉬기
  아래 기록에 보낸 명령(→)과 받은 응답(←)이 나옵니다.
  연결되어 있는 동안 30초마다 @PING을 스스로 보냅니다.
  끝내기 전에 "알림 끄기"를 누릅니다. 켜 둔 알림은 수신기에 남아 다음에 연결할 때 다시 깜빡입니다.

명령줄로 쓰기
  이 폴더에서 PowerShell이나 명령 프롬프트를 엽니다. PowerShell에서는 앞에 .\ 를 붙입니다.

    .\blue-alert-sender -cli -addr AA:BB:CC:DD:EE:FF ping
    .\blue-alert-sender -cli -addr AA:BB:CC:DD:EE:FF test
    .\blue-alert-sender -cli -addr AA:BB:CC:DD:EE:FF -hold 60 alert fast
    .\blue-alert-sender -cli -addr AA:BB:CC:DD:EE:FF bright 30
    .\blue-alert-sender -cli -addr AA:BB:CC:DD:EE:FF off

  명령: ping, test, alert [slow|fast|on] [초], off, sleep, bright <1~100>, raw <수신기 명령>
  -hold <초>  보낸 뒤 그 시간만큼 연결을 유지합니다. 알림이 깜빡이는 것을 보려면 필요합니다.
  -addr 를 빼면 마지막으로 연결에 성공한 주소를 씁니다.
  전체 도움말: .\blue-alert-sender -cli help
  종료 코드: 0 성공, 1 실패, 2 사용법이 틀림

주소를 모를 때
    .\blue-alert-sender -cli scan
  주변의 BLE 기기를 찾아 주소를 보여 줍니다. 수신기는 다른 기기와 연결되어 있지 않을 때만 보입니다.

잘 안 될 때
  연결하지 못했습니다 (Unreachable)
      수신기가 꺼져 있거나, 멀거나, 다른 기기(휴대폰 등)가 연결해 있습니다.
  이 PC의 다른 프로그램이 수신기를 쓰고 있습니다
      UI 창이 떠 있으면 닫고 다시 합니다. 한 PC에서 한 프로그램만 쓸 수 있습니다.
  수신기 신호가 잡히지 않습니다
      주소, 수신기 전원, PC의 Bluetooth를 확인합니다.
  PowerShell에서 'blue-alert-sender' is not recognized
      .\blue-alert-sender 처럼 앞에 .\ 를 붙입니다.
  실행이 막힘 ("Windows의 PC 보호")
      서명되지 않은 실행 파일입니다. 회사 PC에서는 우회하지 말고 담당자와 상의합니다.

버전은 .\blue-alert-sender -version 으로 볼 수 있습니다.
