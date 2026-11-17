//! UI 뒤에서 블루투스를 맡는 스레드.
//!
//! 연결과 명령은 몇 초씩 걸릴 수 있어서 화면을 그리는 스레드에서 하지 않는다.
//! UI는 `Request`를 보내고, 일어난 일은 `Event`로 돌려받는다.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use crate::ble::{self, Link};
use crate::protocol;

/// 연결해 있는 동안 `@PING`을 보내는 간격. 수신기는 90초 동안 아무것도 못 받으면
/// 연결이 살아 있지 않다고 보고 알림을 가린다.
const PING_INTERVAL: Duration = Duration::from_secs(30);
/// 알림(Notify)을 못 쓸 때 TX 값을 읽어 보는 간격.
const POLL_INTERVAL: Duration = Duration::from_secs(1);
/// Windows가 "끊김"이라고 한 뒤 이만큼 이어지면 연결을 놓는다.
const LOST_AFTER: Duration = Duration::from_secs(3);

pub enum Request {
    Connect(u64),
    Disconnect,
    /// 명령을 보낸다. 연결되어 있지 않으면 먼저 그 주소로 연결한다.
    Send { address: u64, command: String },
}

#[derive(Clone, Debug, PartialEq)]
pub enum LinkState {
    Disconnected,
    Connecting,
    Connected { name: String },
}

pub enum Event {
    State(LinkState),
    /// 진행 상황 같은 알림 글.
    Notice(String),
    /// 실패. 사람이 읽을 이유를 담는다.
    Problem(String),
    /// 명령을 보냈다.
    Sent(String),
    /// 그 명령의 응답.
    Reply(String),
    /// 명령과 상관없이 온 응답(확인 버튼의 ACK 등).
    Received(String),
    /// 자동으로 보낸 `@PING`에 `PONG`이 왔다.
    Pong,
}

pub struct Worker {
    requests: Sender<Request>,
    pub events: Receiver<Event>,
}

impl Worker {
    /// 스레드를 시작한다. `wake`는 새 `Event`가 생길 때마다 불려 화면을 다시 그리게 한다.
    pub fn spawn(wake: impl Fn() + Send + 'static) -> Worker {
        let (requests, request_rx) = mpsc::channel::<Request>();
        let (event_tx, events) = mpsc::channel::<Event>();
        std::thread::Builder::new()
            .name("bluetooth".to_owned())
            .spawn(move || {
                ble::init_thread();
                let emit = move |event: Event| {
                    let _ = event_tx.send(event);
                    wake();
                };
                run(request_rx, &emit);
            })
            .expect("블루투스 스레드를 시작하지 못함");
        Worker { requests, events }
    }

    pub fn request(&self, request: Request) {
        let _ = self.requests.send(request);
    }
}

struct Session {
    link: Link,
    next_ping: Instant,
    next_poll: Instant,
    /// Windows가 끊김이라고 하기 시작한 때.
    lost_since: Option<Instant>,
}

fn run(requests: Receiver<Request>, emit: &dyn Fn(Event)) {
    let mut session: Option<Session> = None;
    loop {
        match requests.recv_timeout(Duration::from_millis(200)) {
            Ok(Request::Connect(address)) => {
                drop(session.take()); // 앞의 연결을 먼저 놓는다
                session = connect(address, emit);
            }
            Ok(Request::Disconnect) => {
                if session.take().is_some() {
                    emit(Event::Notice("연결을 끊었습니다.".to_owned()));
                }
                emit(Event::State(LinkState::Disconnected));
            }
            Ok(Request::Send { address, command }) => {
                if session.is_none() {
                    session = connect(address, emit);
                }
                if let Some(active) = session.as_mut() {
                    emit(Event::Sent(command.clone()));
                    match active.link.send(&command) {
                        Ok(reply) => {
                            emit(Event::Reply(reply));
                            active.next_ping = Instant::now() + PING_INTERVAL;
                        }
                        Err(err) => drop_session(&mut session, &err.to_string(), emit),
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break, // UI가 닫혔다
        }
        if let Some(active) = session.as_mut() {
            if let Err(reason) = tick(active, emit) {
                drop_session(&mut session, &reason, emit);
            }
        }
    }
}

fn connect(address: u64, emit: &dyn Fn(Event)) -> Option<Session> {
    emit(Event::State(LinkState::Connecting));
    emit(Event::Notice(format!("{}에 연결합니다...", protocol::format_address(address))));
    let mut link = match Link::connect(address, &mut |message| emit(Event::Notice(message.to_owned()))) {
        Ok(link) => link,
        Err(err) => {
            emit(Event::Problem(err.to_string()));
            emit(Event::State(LinkState::Disconnected));
            return None;
        }
    };
    emit(Event::State(LinkState::Connected { name: link.name().to_owned() }));
    emit(Event::Notice(format!("연결됨: {}", link.name())));
    // 연결됐는지는 @PING의 PONG으로 확인한다(수신기의 READY는 알림으로 받지 못할 수 있다).
    emit(Event::Sent("@PING".to_owned()));
    match link.send("@PING") {
        Ok(reply) => emit(Event::Reply(reply)),
        Err(err) => {
            emit(Event::Problem(err.to_string()));
            emit(Event::State(LinkState::Disconnected));
            return None;
        }
    }
    let now = Instant::now();
    Some(Session { link, next_ping: now + PING_INTERVAL, next_poll: now + POLL_INTERVAL, lost_since: None })
}

/// 연결해 있는 동안 0.2초마다 하는 일. 연결을 놓아야 하면 이유를 돌려준다.
fn tick(active: &mut Session, emit: &dyn Fn(Event)) -> Result<(), String> {
    let now = Instant::now();

    if active.link.is_connected() {
        active.lost_since = None;
    } else {
        let since = *active.lost_since.get_or_insert(now);
        if now.duration_since(since) >= LOST_AFTER {
            return Err(ble::BleError::Lost.to_string());
        }
        return Ok(()); // 잠깐 끊긴 것일 수 있으니 조금 기다려 본다
    }

    if active.link.uses_notify() || now >= active.next_poll {
        active.next_poll = now + POLL_INTERVAL;
        for event in active.link.take_events().map_err(|err| err.to_string())? {
            emit(Event::Received(event));
        }
    }

    if now >= active.next_ping {
        active.next_ping = now + PING_INTERVAL;
        let reply = active.link.send("@PING").map_err(|err| err.to_string())?;
        if reply == "PONG" {
            emit(Event::Pong);
        } else {
            emit(Event::Received(reply));
        }
    }
    Ok(())
}

fn drop_session(session: &mut Option<Session>, reason: &str, emit: &dyn Fn(Event)) {
    *session = None;
    emit(Event::Problem(reason.to_owned()));
    emit(Event::State(LinkState::Disconnected));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 실제 블루투스를 쓰는 시험이라 평소에는 건너뛴다: `cargo test -- --ignored`
    /// 수신기가 없는 주소로 연결하면, 실패를 알리고 "연결 안 됨"으로 돌아와야 한다.
    #[test]
    #[ignore = "이 PC의 블루투스를 실제로 씀 (15초쯤 걸림)"]
    fn reports_failure_for_an_absent_receiver() {
        let worker = Worker::spawn(|| {});
        worker.request(Request::Send { address: 0x0200_0000_00FF, command: "@PING".to_owned() });
        let mut states = Vec::new();
        let mut problems = Vec::new();
        let mut sent = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(60);
        while Instant::now() < deadline {
            match worker.events.recv_timeout(Duration::from_secs(1)) {
                Ok(Event::State(state)) => {
                    let done = state == LinkState::Disconnected;
                    states.push(state);
                    if done {
                        break;
                    }
                }
                Ok(Event::Problem(text)) => problems.push(text),
                Ok(Event::Sent(command)) => sent.push(command),
                Ok(_) | Err(_) => {}
            }
        }
        assert_eq!(states, vec![LinkState::Connecting, LinkState::Disconnected]);
        assert_eq!(problems.len(), 1, "실패 이유가 한 번 나와야 함: {problems:?}");
        assert!(sent.is_empty(), "연결하지 못했으면 명령을 보내지 않아야 함");
    }

    /// 수신기가 켜져 있을 때만 통과한다. 주소는 환경 변수 BLUE_ALERT_SENDER_ADDR로 준다.
    #[test]
    #[ignore = "켜져 있는 수신기가 필요함"]
    fn sends_commands_to_a_live_receiver() {
        let address = std::env::var("BLUE_ALERT_SENDER_ADDR").expect("BLUE_ALERT_SENDER_ADDR에 수신기 주소를 넣으세요");
        let address = protocol::parse_address(&address).unwrap();
        let worker = Worker::spawn(|| {});
        worker.request(Request::Send { address, command: "@TEST".to_owned() });
        let mut replies = Vec::new();
        let mut connected = false;
        let deadline = Instant::now() + Duration::from_secs(60);
        while Instant::now() < deadline && replies.len() < 2 {
            match worker.events.recv_timeout(Duration::from_secs(1)) {
                Ok(Event::State(LinkState::Connected { .. })) => connected = true,
                Ok(Event::Reply(reply)) => replies.push(reply),
                Ok(Event::Problem(text)) => panic!("실패: {text}"),
                Ok(_) | Err(_) => {}
            }
        }
        assert!(connected);
        // 연결 직후의 @PING과, 요청한 @TEST
        assert_eq!(replies, vec!["PONG".to_owned(), "OK".to_owned()]);
        worker.request(Request::Disconnect);
    }
}
