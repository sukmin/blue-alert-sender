//! 수신기와의 블루투스(BLE) 연결.
//!
//! Windows의 `Windows.Devices.Bluetooth`를 직접 쓴다. 페어링은 하지 않고 주소로 연결한다.
//! 수신기는 Nordic UART Service(NUS)로 명령을 받는다: RX에 쓰면 명령, TX가 응답.
//!
//! 알아 둘 것 (집 PC에서 실물로 확인한 것):
//! - Windows는 최근에 신호를 본 기기만 주소로 열 수 있다. 처음 보는 주소는 "없음"이 돌아오므로
//!   한 번 검색한 뒤 다시 연다.
//! - 수신기가 꺼져 있어도 장치는 열리고, 서비스를 얻는 단계에서 Unreachable이 나온다.
//! - 같은 PC의 다른 프로그램이 수신기에 연결해 있으면 서비스를 얻는 단계에서 AccessDenied가 나온다.
//! - 응답은 TX 값으로 남아 있어 읽어 올 수 있다. 알림(Notify)을 켜면 바로 받는다.

use std::fmt;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use windows::core::{Error as WinError, Interface, RuntimeType, GUID};
use windows::Devices::Bluetooth::Advertisement::{
    BluetoothLEAdvertisementReceivedEventArgs, BluetoothLEAdvertisementWatcher,
    BluetoothLEScanningMode,
};
use windows::Devices::Bluetooth::GenericAttributeProfile::{
    GattCharacteristic, GattClientCharacteristicConfigurationDescriptorValue,
    GattCommunicationStatus, GattDeviceService, GattValueChangedEventArgs,
};
use windows::Devices::Bluetooth::{BluetoothCacheMode, BluetoothConnectionStatus, BluetoothLEDevice};
use windows::Foundation::TypedEventHandler;
use windows::Storage::Streams::{DataReader, DataWriter, IBuffer};
use windows_future::{AsyncOperationCompletedHandler, IAsyncInfo, IAsyncOperation};

/// NUS 서비스와 두 특성의 UUID.
const NUS_SERVICE: GUID = GUID::from_u128(0x6E400001_B5A3_F393_E0A9_E50E24DCCA9E);
const NUS_RX: GUID = GUID::from_u128(0x6E400002_B5A3_F393_E0A9_E50E24DCCA9E); // PC → 수신기 (쓰기)
const NUS_TX: GUID = GUID::from_u128(0x6E400003_B5A3_F393_E0A9_E50E24DCCA9E); // 수신기 → PC (읽기·알림)

const OPEN_TIMEOUT: Duration = Duration::from_secs(10);
/// 수신기가 꺼져 있을 때 Unreachable이 돌아오기까지 8초쯤 걸렸다. 넉넉히 기다린다.
const SERVICE_TIMEOUT: Duration = Duration::from_secs(30);
const IO_TIMEOUT: Duration = Duration::from_secs(10);
/// 처음 보는 주소일 때 신호를 기다리는 시간.
const SEARCH_TIMEOUT: Duration = Duration::from_secs(15);
/// 알림으로 응답이 오기를 기다리는 시간. 안 오면 TX 값을 읽는다.
const REPLY_TIMEOUT: Duration = Duration::from_millis(1500);
/// 알림을 쓰지 못할 때, 쓰고 나서 읽기까지 기다리는 시간(스크립트와 같은 값).
const READ_DELAY: Duration = Duration::from_millis(300);

#[derive(Debug)]
pub enum BleError {
    /// 검색해도 그 주소의 신호가 없다.
    NotFound,
    /// 장치는 알지만 지금 닿지 않는다(꺼짐, 멀리 있음, 다른 기기가 연결함).
    Unreachable(String),
    /// 그 주소의 기기에 NUS 서비스가 없다.
    NoService,
    /// 정한 시간 안에 답이 없다.
    Timeout(&'static str),
    /// 쓰기나 읽기가 실패했다.
    Io(&'static str, String),
    /// 이어져 있던 연결이 끊겼다.
    Lost,
    /// 이 PC의 다른 프로그램이 수신기의 서비스를 쓰고 있다(Windows가 AccessDenied를 돌려줌).
    Busy,
    /// Windows가 돌려준 그 밖의 오류.
    Windows(WinError),
}

impl fmt::Display for BleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BleError::NotFound => write!(
                f,
                "수신기 신호가 잡히지 않습니다. 주소, 수신기 전원, 거리(2~3m), PC의 Bluetooth가 켜져 있는지 확인하세요."
            ),
            BleError::Unreachable(status) => write!(
                f,
                "연결하지 못했습니다 ({status}). 수신기 전원, 거리, 다른 기기가 연결해 있지 않은지 확인하세요."
            ),
            BleError::NoService => write!(f, "NUS 서비스가 없습니다. 주소가 수신기의 것인지 확인하세요."),
            BleError::Timeout(what) => write!(f, "응답 시간 초과 ({what})"),
            BleError::Io(what, status) => write!(f, "{what} 실패 ({status})"),
            BleError::Lost => write!(f, "수신기와의 연결이 끊겼습니다. 수신기 전원과 거리를 확인하세요."),
            BleError::Busy => write!(
                f,
                "이 PC의 다른 프로그램이 수신기를 쓰고 있습니다. 그 프로그램(blue-alert-sender 창 등)을 닫고 다시 하세요."
            ),
            BleError::Windows(err) => write!(f, "Windows 오류: {}", err.message()),
        }
    }
}

impl From<WinError> for BleError {
    fn from(err: WinError) -> Self {
        BleError::Windows(err)
    }
}

pub type Result<T> = std::result::Result<T, BleError>;

/// 이 스레드에서 Windows 런타임을 쓸 준비를 한다. 여러 번 불러도 된다.
pub fn init_thread() {
    use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};
    // 이미 준비되어 있거나 다른 방식으로 준비된 스레드면 오류가 오지만 그대로 써도 된다.
    let _ = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };
}

/// 비동기 작업이 끝나기를 `timeout`까지 기다려 결과를 얻는다.
fn wait<T: RuntimeType + 'static>(op: IAsyncOperation<T>, timeout: Duration, what: &'static str) -> Result<T> {
    let (done_tx, done_rx) = mpsc::channel::<()>();
    op.SetCompleted(&AsyncOperationCompletedHandler::new(move |_, _| {
        let _ = done_tx.send(());
        Ok(())
    }))?;
    match done_rx.recv_timeout(timeout) {
        Ok(()) => Ok(op.GetResults()?),
        Err(_) => {
            if let Ok(info) = op.cast::<IAsyncInfo>() {
                let _ = info.Cancel();
            }
            Err(BleError::Timeout(what))
        }
    }
}

fn buffer_to_bytes(buffer: &IBuffer) -> Result<Vec<u8>> {
    let len = buffer.Length()? as usize;
    let mut bytes = vec![0u8; len];
    if len > 0 {
        DataReader::FromBuffer(buffer)?.ReadBytes(&mut bytes)?;
    }
    Ok(bytes)
}

fn bytes_to_buffer(bytes: &[u8]) -> Result<IBuffer> {
    let writer = DataWriter::new()?;
    writer.WriteBytes(bytes)?;
    Ok(writer.DetachBuffer()?)
}

/// TX 값 하나를 줄 단위 응답으로 나눈다. 수신기는 응답 끝에 줄바꿈을 붙인다.
fn split_replies(bytes: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(bytes)
        .split('\n')
        .map(|line| line.trim().to_owned())
        .filter(|line| !line.is_empty())
        .collect()
}

/// 주변에서 신호를 낸 BLE 기기 하나.
#[derive(Clone, Debug)]
pub struct Found {
    pub address: u64,
    pub name: String,
    /// 신호 세기(dBm). 0에 가까울수록 가깝다.
    pub rssi: i16,
    /// 광고에 NUS 서비스가 들어 있는지(수신기일 가능성이 높음).
    pub has_nus: bool,
}

/// `duration` 동안 주변의 BLE 광고를 듣는다. `stop_at`의 주소가 보이면 바로 멈춘다.
pub fn scan(duration: Duration, stop_at: Option<u64>) -> Result<Vec<Found>> {
    let watcher = BluetoothLEAdvertisementWatcher::new()?;
    // Active: 이름이 스캔 응답에 들어 있는 기기의 이름도 받는다.
    watcher.SetScanningMode(BluetoothLEScanningMode::Active)?;
    let (seen_tx, seen_rx) = mpsc::channel::<Found>();
    let handler = TypedEventHandler::<BluetoothLEAdvertisementWatcher, BluetoothLEAdvertisementReceivedEventArgs>::new(
        move |_, args| {
            if let Some(args) = args.as_ref() {
                let advertisement = args.Advertisement()?;
                let has_nus = advertisement
                    .ServiceUuids()
                    .map(|uuids| uuids.into_iter().any(|uuid| uuid == NUS_SERVICE))
                    .unwrap_or(false);
                let _ = seen_tx.send(Found {
                    address: args.BluetoothAddress()?,
                    name: advertisement.LocalName().map(|n| n.to_string()).unwrap_or_default(),
                    rssi: args.RawSignalStrengthInDBm().unwrap_or(0),
                    has_nus,
                });
            }
            Ok(())
        },
    );
    let token = watcher.Received(&handler)?;
    watcher.Start()?;

    let mut found: Vec<Found> = Vec::new();
    let deadline = Instant::now() + duration;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        match seen_rx.recv_timeout(left) {
            Ok(item) => {
                let hit = stop_at == Some(item.address);
                match found.iter_mut().find(|f| f.address == item.address) {
                    Some(known) => {
                        known.rssi = item.rssi;
                        known.has_nus |= item.has_nus;
                        if !item.name.is_empty() {
                            known.name = item.name;
                        }
                    }
                    None => found.push(item),
                }
                if hit {
                    break;
                }
            }
            Err(RecvTimeoutError::Timeout) => break,
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    let _ = watcher.Stop();
    let _ = watcher.RemoveReceived(token);
    Ok(found)
}

/// 수신기와 이어진 연결 하나. 버리면(drop) 끊긴다.
pub struct Link {
    device: BluetoothLEDevice,
    service: GattDeviceService,
    rx: GattCharacteristic,
    tx: GattCharacteristic,
    name: String,
    /// 알림(Notify)을 켰을 때의 등록 번호. 못 켰으면 `None`이고 읽기로만 응답을 얻는다.
    notify_token: Option<i64>,
    notified: Receiver<String>,
    /// 명령의 응답을 기다리다 받은, 명령과 상관없는 응답(ACK 등).
    stray: Vec<String>,
    /// 읽기 방식일 때 마지막으로 본 TX 값.
    last_read: String,
}

impl Link {
    /// 주소로 연결한다. `progress`에는 진행 상황을 알리는 글이 온다.
    pub fn connect(address: u64, progress: &mut dyn FnMut(&str)) -> Result<Link> {
        let device = match open_device(address) {
            Some(device) => device,
            None => {
                progress("이 PC는 아직 수신기를 본 적이 없습니다. 주변을 검색합니다(최대 15초)...");
                let found = scan(SEARCH_TIMEOUT, Some(address))?;
                if !found.iter().any(|f| f.address == address) {
                    return Err(BleError::NotFound);
                }
                progress("수신기 신호를 찾았습니다.");
                // 신호를 본 직후에는 Windows가 아직 장치를 만들지 못했을 수 있어 몇 번 다시 연다.
                let mut opened = None;
                for _ in 0..5 {
                    opened = open_device(address);
                    if opened.is_some() {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(400));
                }
                opened.ok_or(BleError::NotFound)?
            }
        };
        let name = device.Name().map(|n| n.to_string()).unwrap_or_default();

        // 여기서 실제 무선 연결이 이루어진다.
        let services = wait(device.GetGattServicesForUuidAsync(NUS_SERVICE)?, SERVICE_TIMEOUT, "서비스 찾기");
        let services = match services {
            Ok(services) => services,
            Err(err) => {
                let _ = device.Close();
                return Err(err);
            }
        };
        let status = services.Status()?;
        if status != GattCommunicationStatus::Success {
            let _ = device.Close();
            return Err(unreachable(status));
        }
        let list = services.Services()?;
        if list.Size()? == 0 {
            let _ = device.Close();
            return Err(BleError::NoService);
        }
        let service = list.GetAt(0)?;

        let characteristics = (|| -> Result<(GattCharacteristic, GattCharacteristic)> {
            Ok((find_characteristic(&service, NUS_RX)?, find_characteristic(&service, NUS_TX)?))
        })();
        let (rx, tx) = match characteristics {
            Ok(pair) => pair,
            Err(err) => {
                let _ = service.Close();
                let _ = device.Close();
                return Err(err);
            }
        };

        // 알림을 켠다. 실패해도 읽기 방식으로 계속 쓸 수 있다.
        let (notify_tx, notified) = mpsc::channel::<String>();
        let notify_token = enable_notify(&tx, notify_tx);
        if notify_token.is_none() {
            progress("알림(Notify)을 켜지 못해 응답을 읽기 방식으로 받습니다.");
        }

        let mut link = Link {
            device,
            service,
            rx,
            tx,
            name,
            notify_token,
            notified,
            stray: Vec::new(),
            last_read: String::new(),
        };
        // 연결 직후의 TX 값(READY,3.1 또는 앞 연결의 마지막 응답)을 기준으로 삼는다.
        link.last_read = link.read_tx().unwrap_or_default();
        Ok(link)
    }

    /// 수신기의 이름(`BlueAlert-01`).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 응답을 알림으로 바로 받고 있는지. 아니면 1초마다 읽어야 ACK를 알 수 있다.
    pub fn uses_notify(&self) -> bool {
        self.notify_token.is_some()
    }

    /// Windows가 보기에 지금 연결되어 있는지.
    pub fn is_connected(&self) -> bool {
        matches!(self.device.ConnectionStatus(), Ok(BluetoothConnectionStatus::Connected))
    }

    /// 명령 하나를 보내고 그 응답을 돌려준다.
    pub fn send(&mut self, command: &str) -> Result<String> {
        // 앞서 온 알림(ACK 등)이 이 명령의 응답으로 잘못 읽히지 않게 먼저 치워 둔다.
        while let Ok(line) = self.notified.try_recv() {
            self.stray.push(line);
        }
        let buffer = bytes_to_buffer(command.as_bytes())?;
        let status = wait(self.rx.WriteValueAsync(&buffer)?, IO_TIMEOUT, "명령 쓰기")?;
        if status != GattCommunicationStatus::Success {
            return Err(BleError::Io("명령 쓰기", status_name(status)));
        }

        if self.notify_token.is_some() {
            let deadline = Instant::now() + REPLY_TIMEOUT;
            loop {
                let left = deadline.saturating_duration_since(Instant::now());
                match self.notified.recv_timeout(left) {
                    Ok(line) if crate::protocol::is_unsolicited(&line) => self.stray.push(line),
                    Ok(line) => {
                        self.last_read = line.clone();
                        return Ok(line);
                    }
                    Err(_) => break, // 알림이 오지 않았다. 아래에서 읽는다.
                }
            }
        } else {
            std::thread::sleep(READ_DELAY);
        }
        let reply = self.read_tx()?;
        self.last_read = reply.clone();
        Ok(reply)
    }

    /// 명령과 상관없이 온 응답(확인 버튼의 ACK 등)을 꺼낸다. 없으면 빈 목록.
    ///
    /// 읽기 방식일 때는 TX 값을 한 번 읽어 바뀌었는지 본다.
    pub fn take_events(&mut self) -> Result<Vec<String>> {
        let mut events = std::mem::take(&mut self.stray);
        if self.notify_token.is_some() {
            while let Ok(line) = self.notified.try_recv() {
                events.push(line);
            }
        } else {
            let now = self.read_tx()?;
            if now != self.last_read {
                self.last_read = now.clone();
                if !now.is_empty() {
                    events.push(now);
                }
            }
        }
        Ok(events)
    }

    /// TX 값(수신기의 마지막 응답)을 읽는다.
    fn read_tx(&self) -> Result<String> {
        let read = wait(
            self.tx.ReadValueWithCacheModeAsync(BluetoothCacheMode::Uncached)?,
            IO_TIMEOUT,
            "응답 읽기",
        )?;
        let status = read.Status()?;
        if status != GattCommunicationStatus::Success {
            return Err(BleError::Io("응답 읽기", status_name(status)));
        }
        let bytes = buffer_to_bytes(&read.Value()?)?;
        Ok(split_replies(&bytes).pop().unwrap_or_default())
    }
}

impl Drop for Link {
    fn drop(&mut self) {
        if let Some(token) = self.notify_token.take() {
            let _ = self.tx.RemoveValueChanged(token);
        }
        // 서비스와 장치를 닫아야 Windows가 무선 연결을 놓는다.
        let _ = self.service.Close();
        let _ = self.device.Close();
    }
}

/// 주소로 장치를 연다. Windows가 모르는 주소면 `None`.
fn open_device(address: u64) -> Option<BluetoothLEDevice> {
    let op = BluetoothLEDevice::FromBluetoothAddressAsync(address).ok()?;
    wait(op, OPEN_TIMEOUT, "장치 열기").ok()
}

fn find_characteristic(service: &GattDeviceService, uuid: GUID) -> Result<GattCharacteristic> {
    let result = wait(service.GetCharacteristicsForUuidAsync(uuid)?, IO_TIMEOUT, "특성 찾기")?;
    let status = result.Status()?;
    if status != GattCommunicationStatus::Success {
        return Err(unreachable(status));
    }
    let list = result.Characteristics()?;
    if list.Size()? == 0 {
        return Err(BleError::NoService);
    }
    Ok(list.GetAt(0)?)
}

/// TX 특성의 알림을 켜고, 오는 응답을 `sink`로 보낸다. 켜지 못하면 `None`.
fn enable_notify(tx: &GattCharacteristic, sink: Sender<String>) -> Option<i64> {
    let handler = TypedEventHandler::<GattCharacteristic, GattValueChangedEventArgs>::new(move |_, args| {
        if let Some(args) = args.as_ref() {
            if let Ok(bytes) = args.CharacteristicValue().map_err(BleError::from).and_then(|b| buffer_to_bytes(&b)) {
                for line in split_replies(&bytes) {
                    let _ = sink.send(line);
                }
            }
        }
        Ok(())
    });
    let token = tx.ValueChanged(&handler).ok()?;
    let enabled = tx
        .WriteClientCharacteristicConfigurationDescriptorAsync(GattClientCharacteristicConfigurationDescriptorValue::Notify)
        .map_err(BleError::from)
        .and_then(|op| wait(op, IO_TIMEOUT, "알림 켜기"));
    match enabled {
        Ok(GattCommunicationStatus::Success) => Some(token),
        _ => {
            let _ = tx.RemoveValueChanged(token);
            None
        }
    }
}

/// 연결 단계에서 Success가 아닌 상태를 오류로 바꾼다.
fn unreachable(status: GattCommunicationStatus) -> BleError {
    if status == GattCommunicationStatus::AccessDenied {
        BleError::Busy
    } else {
        BleError::Unreachable(status_name(status))
    }
}

fn status_name(status: GattCommunicationStatus) -> String {
    match status {
        GattCommunicationStatus::Success => "Success".to_owned(),
        GattCommunicationStatus::Unreachable => "Unreachable".to_owned(),
        GattCommunicationStatus::ProtocolError => "ProtocolError".to_owned(),
        GattCommunicationStatus::AccessDenied => "AccessDenied".to_owned(),
        other => format!("상태 {}", other.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_replies_on_newlines() {
        assert_eq!(split_replies(b"PONG\n"), vec!["PONG".to_owned()]);
        assert_eq!(split_replies(b"OK\nPONG\n"), vec!["OK".to_owned(), "PONG".to_owned()]);
        assert_eq!(split_replies(b"READY,3.1\r\n"), vec!["READY,3.1".to_owned()]);
        assert!(split_replies(b"").is_empty());
        assert!(split_replies(b"\n").is_empty());
    }
}
