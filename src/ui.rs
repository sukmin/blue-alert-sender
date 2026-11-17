//! UI 모드: 주소 입력란 하나와 명령 버튼들.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::{Color32, FontFamily, RichText, Vec2};

use crate::protocol;
use crate::settings;
use crate::win::clock;
use crate::worker::{Event, LinkState, Request, Worker};

/// 주소 칸이 비어 있을 때 흐리게 보여 주는 예시. 실제 주소는 수신기마다 다르다.
const ADDRESS_EXAMPLE: &str = "AA:BB:CC:DD:EE:FF";
/// 기록에 남겨 두는 줄 수.
const MAX_LOG_LINES: usize = 300;
const BUTTON_SIZE: Vec2 = Vec2::new(132.0, 34.0);

/// 버튼 하나: 이름, 보내는 명령, 마우스를 올렸을 때의 설명.
struct Action {
    label: &'static str,
    command: &'static str,
    hint: &'static str,
}

const TEST_ACTIONS: &[Action] = &[
    Action { label: "연결 확인", command: "@PING", hint: "수신기가 PONG으로 답하는지 봅니다." },
    Action { label: "시험 깜빡임", command: "@TEST", hint: "LED를 3번 깜빡입니다." },
];
const ALERT_ACTIONS: &[Action] = &[
    Action { label: "느린 알림", command: "@ALERT,SLOW,0", hint: "1초에 1번 깜빡입니다. 끌 때까지 계속합니다." },
    Action { label: "빠른 알림", command: "@ALERT,FAST,0", hint: "1초에 2.5번 깜빡입니다. 끌 때까지 계속합니다." },
    Action { label: "계속 켜짐", command: "@ALERT,ON,0", hint: "깜빡이지 않고 켜 둡니다. 끌 때까지 계속합니다." },
    Action { label: "빠른 알림 10초", command: "@ALERT,FAST,10", hint: "10초 동안 빠르게 깜빡이고 스스로 꺼집니다." },
    Action { label: "알림 끄기", command: "@OFF", hint: "켜져 있는 알림을 모두 끕니다." },
];
const BRIGHT_ACTIONS: &[Action] = &[
    Action { label: "밝기 30%", command: "@BRIGHT,30", hint: "LED 밝기를 30%로 합니다. 전원을 다시 넣으면 100%로 돌아갑니다." },
    Action { label: "밝기 60%", command: "@BRIGHT,60", hint: "LED 밝기를 60%로 합니다." },
    Action { label: "밝기 100%", command: "@BRIGHT,100", hint: "LED 밝기를 100%로 합니다." },
];
const SLEEP_ACTIONS: &[Action] = &[Action {
    label: "쉬기",
    command: "@SLEEP",
    hint: "알림을 모두 지우고 LED를 끕니다. 알림 명령을 받거나 다시 연결되면 풀립니다.",
}];

#[derive(Clone, Copy, PartialEq)]
enum LogKind {
    Sent,
    Received,
    Info,
    Problem,
}

struct LogLine {
    time: String,
    kind: LogKind,
    text: String,
}

pub fn run(prefill: Option<String>) -> eframe::Result {
    // 주소는 명령줄(-addr), 없으면 지난번에 연결에 성공한 주소. 둘 다 없으면 빈칸으로 시작한다.
    let address = prefill.or_else(settings::load_address).unwrap_or_default();
    let viewport = egui::ViewportBuilder::default()
        .with_title(crate::APP_NAME)
        .with_app_id("blue-alert-sender")
        .with_inner_size([470.0, 700.0])
        .with_min_inner_size([420.0, 460.0])
        .with_icon(window_icon());
    let options = eframe::NativeOptions { viewport, centered: true, ..Default::default() };
    eframe::run_native(
        crate::APP_NAME,
        options,
        Box::new(move |cc| {
            install_fonts(&cc.egui_ctx);
            let ctx = cc.egui_ctx.clone();
            let worker = Worker::spawn(move || ctx.request_repaint());
            Ok(Box::new(App {
                address,
                worker,
                state: LinkState::Disconnected,
                log: Vec::new(),
                last_pong: None,
            }))
        }),
    )
}

struct App {
    address: String,
    worker: Worker,
    state: LinkState,
    log: Vec<LogLine>,
    /// 마지막으로 PONG을 받은 때. 연결이 살아 있는지 보여 주는 데 쓴다.
    last_pong: Option<Instant>,
}

impl App {
    fn push_log(&mut self, kind: LogKind, text: impl Into<String>) {
        self.log.push(LogLine { time: clock(), kind, text: text.into() });
        if self.log.len() > MAX_LOG_LINES {
            let extra = self.log.len() - MAX_LOG_LINES;
            self.log.drain(..extra);
        }
    }

    /// 작업 스레드에서 온 일들을 화면 상태에 반영한다.
    fn drain_events(&mut self) {
        while let Ok(event) = self.worker.events.try_recv() {
            match event {
                Event::State(state) => {
                    match &state {
                        LinkState::Disconnected => self.last_pong = None,
                        // 연결에 성공한 주소만 기억한다. 잘못 적은 주소가 남으면 다음에 CLI가 그 주소로 간다.
                        LinkState::Connected { .. } => {
                            if let Ok(address) = protocol::parse_address(&self.address) {
                                settings::save_address(&protocol::format_address(address));
                            }
                        }
                        LinkState::Connecting => {}
                    }
                    self.state = state;
                }
                Event::Notice(text) => self.push_log(LogKind::Info, text),
                Event::Problem(text) => self.push_log(LogKind::Problem, text),
                Event::Sent(command) => self.push_log(LogKind::Sent, command),
                Event::Reply(reply) => {
                    if reply == "PONG" {
                        self.last_pong = Some(Instant::now());
                    }
                    self.push_reply(reply);
                }
                Event::Received(reply) => self.push_reply(reply),
                Event::Pong => self.last_pong = Some(Instant::now()),
            }
        }
    }

    fn push_reply(&mut self, reply: String) {
        let meaning = protocol::describe_reply(&reply);
        let kind = if protocol::is_success(&reply) { LogKind::Received } else { LogKind::Problem };
        let text = match (reply.is_empty(), meaning.is_empty()) {
            (true, _) => "(응답 없음)".to_owned(),
            (false, true) => reply,
            (false, false) => format!("{reply}  — {meaning}"),
        };
        self.push_log(kind, text);
    }

    fn address_bar(&mut self, ui: &mut egui::Ui) {
        let parsed = protocol::parse_address(&self.address);
        let idle = self.state == LinkState::Disconnected;
        ui.horizontal(|ui| {
            ui.label("수신기 주소");
            ui.add_enabled(
                idle,
                egui::TextEdit::singleline(&mut self.address)
                    .font(egui::TextStyle::Monospace)
                    .desired_width(170.0)
                    .hint_text(ADDRESS_EXAMPLE),
            )
            .on_hover_text("수신기의 블루투스 주소입니다. 예: AA:BB:CC:DD:EE:FF");
            match &self.state {
                LinkState::Disconnected => {
                    let button = ui.add_enabled(parsed.is_ok(), egui::Button::new("연결").min_size(Vec2::new(72.0, 26.0)));
                    if button.clicked() {
                        if let Ok(address) = parsed {
                            self.worker.request(Request::Connect(address));
                        }
                    }
                }
                LinkState::Connecting => {
                    ui.add_enabled(false, egui::Button::new("연결하는 중…").min_size(Vec2::new(72.0, 26.0)));
                }
                LinkState::Connected { .. } => {
                    if ui.add(egui::Button::new("끊기").min_size(Vec2::new(72.0, 26.0))).clicked() {
                        self.worker.request(Request::Disconnect);
                    }
                }
            }
        });
        if let (Err(reason), true) = (&parsed, idle) {
            if self.address.trim().is_empty() {
                // 아직 아무것도 적지 않은 것은 잘못이 아니므로 빨간 글씨 대신 안내만 한다.
                let hint = format!("수신기의 블루투스 주소를 적어 주세요. 예: {ADDRESS_EXAMPLE}");
                ui.label(RichText::new(hint).small().color(ui.visuals().weak_text_color()));
            } else {
                ui.label(RichText::new(reason).small().color(problem_color(ui)));
            }
        }
    }

    fn status_line(&self, ui: &mut egui::Ui) {
        let (color, text) = match &self.state {
            LinkState::Disconnected => (Color32::from_gray(140), "연결 안 됨".to_owned()),
            LinkState::Connecting => (Color32::from_rgb(0xE0, 0xA0, 0x20), "연결하는 중…".to_owned()),
            LinkState::Connected { name } => {
                let pong = match self.last_pong {
                    Some(at) => format!(" · 마지막 PONG {}초 전", at.elapsed().as_secs()),
                    None => String::new(),
                };
                let name = if name.is_empty() { "(이름을 받지 못함)" } else { name.as_str() };
                (Color32::from_rgb(0x2E, 0xA0, 0x4F), format!("연결됨: {name}{pong}"))
            }
        };
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(14.0), egui::Sense::hover());
            ui.painter().circle_filled(rect.center(), 5.5, color);
            ui.label(text);
        });
    }

    /// 버튼 한 묶음을 그리고, 누른 버튼의 명령을 보낸다.
    fn action_group(&mut self, ui: &mut egui::Ui, title: &str, actions: &[Action]) {
        ui.add_space(10.0);
        ui.label(RichText::new(title).strong());
        ui.add_space(2.0);
        let address = protocol::parse_address(&self.address);
        let enabled = address.is_ok() && self.state != LinkState::Connecting;
        let mut clicked: Option<&'static str> = None;
        ui.horizontal_wrapped(|ui| {
            for action in actions {
                let button = egui::Button::new(action.label).min_size(BUTTON_SIZE);
                let response = ui
                    .add_enabled(enabled, button)
                    .on_hover_text(format!("{}\n보내는 명령: {}", action.hint, action.command));
                if response.clicked() {
                    clicked = Some(action.command);
                }
            }
        });
        if let (Some(command), Ok(address)) = (clicked, address) {
            self.worker.request(Request::Send { address, command: command.to_owned() });
        }
    }

    fn log_panel(&mut self, ui: &mut egui::Ui) {
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("기록").strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("지우기").clicked() {
                    self.log.clear();
                }
            });
        });
        ui.add_space(2.0);
        let weak = ui.visuals().weak_text_color();
        let problem = problem_color(ui);
        let sent = ui.visuals().hyperlink_color;
        egui::Frame::group(ui.style()).show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).stick_to_bottom(true).show(ui, |ui| {
                if self.log.is_empty() {
                    ui.label(RichText::new("버튼을 누르면 주고받은 내용이 여기에 나옵니다.").color(weak));
                }
                for line in &self.log {
                    let (mark, color) = match line.kind {
                        LogKind::Sent => ("→", sent),
                        LogKind::Received => ("←", ui.visuals().text_color()),
                        LogKind::Info => ("·", weak),
                        LogKind::Problem => ("!", problem),
                    };
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(&line.time).monospace().color(weak));
                        ui.label(RichText::new(mark).monospace().color(color));
                        ui.label(RichText::new(&line.text).color(color));
                    });
                }
            });
        });
    }
}

impl eframe::App for App {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_events();
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // "마지막 PONG ○초 전"이 올라가도록 연결해 있는 동안에는 1초마다 다시 그린다.
        if matches!(self.state, LinkState::Connected { .. }) {
            ui.ctx().request_repaint_after(Duration::from_secs(1));
        }
        let frame = egui::Frame::central_panel(ui.style()).inner_margin(14);
        egui::CentralPanel::default().frame(frame).show(ui, |ui| {
            self.address_bar(ui);
            ui.add_space(4.0);
            self.status_line(ui);
            ui.label(
                RichText::new("버튼을 누르면 연결되어 있지 않아도 먼저 연결합니다. 연결해 있는 동안에는 30초마다 @PING을 보내, 알림이 수신기에서 계속 보이게 합니다.")
                    .small()
                    .color(ui.visuals().weak_text_color()),
            );
            ui.separator();
            self.action_group(ui, "시험", TEST_ACTIONS);
            self.action_group(ui, "알림", ALERT_ACTIONS);
            self.action_group(ui, "밝기", BRIGHT_ACTIONS);
            self.action_group(ui, "쉬기", SLEEP_ACTIONS);
            self.log_panel(ui);
        });
    }
}

fn problem_color(ui: &egui::Ui) -> Color32 {
    ui.visuals().error_fg_color
}

/// 한글이 나오도록 맑은 고딕을 egui에 넣는다. 글꼴 파일이 없으면 기본 글꼴 그대로 둔다.
fn install_fonts(ctx: &egui::Context) {
    let windows = std::env::var_os("WINDIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("C:\\Windows"));
    let Ok(bytes) = std::fs::read(windows.join("Fonts").join("malgun.ttf")) else {
        return;
    };
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert("malgun".to_owned(), Arc::new(egui::FontData::from_owned(bytes)));
    fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "malgun".to_owned());
    // 고정폭 글꼴에는 한글이 없으므로 뒤에 붙여 한글만 맡긴다.
    fonts.families.entry(FontFamily::Monospace).or_default().push("malgun".to_owned());
    ctx.set_fonts(fonts);
}

/// 창과 작업 표시줄에 쓰는 아이콘: 빨간 LED 모양의 동그라미.
fn window_icon() -> egui::IconData {
    const SIZE: u32 = 64;
    let center = (SIZE as f32 - 1.0) / 2.0;
    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let distance = ((x as f32 - center).powi(2) + (y as f32 - center).powi(2)).sqrt();
            // 가장자리 한 픽셀은 부드럽게
            let alpha = ((28.0 - distance).clamp(0.0, 1.0) * 255.0) as u8;
            // 가운데가 조금 더 밝은 빨강
            let glow = (1.0 - distance / 28.0).clamp(0.0, 1.0);
            rgba.extend_from_slice(&[255, (59.0 + 90.0 * glow * glow) as u8, (48.0 + 70.0 * glow * glow) as u8, alpha]);
        }
    }
    egui::IconData { rgba, width: SIZE, height: SIZE }
}
