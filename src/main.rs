#![deny(clippy::all)]

use iced::{
    executor,
    theme::{self, Theme},
    time,
    widget::{
        button, canvas, checkbox, container, horizontal_space, scrollable, slider, text,
        text_input, Column, Container, Row, Scrollable, Svg,
    },
    window, Alignment, Application, Color, Command, Element, Font, Length, Settings, Subscription,
};

use circle::circle;
use iced_aw::Grid;
use joycon::{Battery, DeviceStatus, ServerStatus};
use needle::Needle;
use settings::WranglerSettings;
use std::{
    io::{
        self,
        prelude::{Read, Write},
    },
    net::SocketAddr,
    time::{Duration, Instant},
};
mod joycon;
mod steam_blacklist;
use steam_blacklist as blacklist;
mod circle;
mod needle;
mod settings;
mod style;
mod update;

const WINDOW_SIZE: (u32, u32) = (980, 700);

pub const ICONS: Font = Font::External {
    name: "Icons",
    bytes: include_bytes!("../assets/icons.ttf"),
};
pub const ICON: &[u8; 16384] = include_bytes!("../assets/icon_64.rgba8");

pub fn main() -> iced::Result {
    /*
    let rgba8 = image_rs::io::Reader::open("assets/icon.png").unwrap().decode().unwrap().to_rgba8();
    std::fs::write("assets/icon_64.rgba8", rgba8.into_raw());
    */
    let settings = Settings {
        window: window::Settings {
            min_size: Some(WINDOW_SIZE),
            size: WINDOW_SIZE,
            icon: window::icon::from_rgba(ICON.to_vec(), 64, 64).ok(),
            ..window::Settings::default()
        },
        antialiasing: true,
        ..Settings::default()
    };
    match MainState::run(settings) {
        Ok(a) => Ok(a),
        Err(e) => {
            println!("{e:?}");
            print!("Press enter to continue...");
            io::stdout().flush().unwrap();
            let _ = io::stdin().read(&mut [0u8]).unwrap();
            Err(e)
        }
    }
}

#[derive(Debug, Clone)]
enum Message {
    SettingsPressed,
    Tick(Instant),
    Dot(Instant),
    AddressChange(String),
    UpdateFound(Option<String>),
    UpdatePressed,
    BlacklistChecked(blacklist::BlacklistResult),
    BlacklistFixPressed,
    JoyconRotate(String, bool),
    JoyconScale(String, f64),
    SettingsResetToggled(bool),
    SettingsIdsToggled(bool),
}

#[derive(Default)]
struct MainState {
    joycon: Option<joycon::Wrapper>,
    joycon_boxes: JoyconBoxes,
    search_dots: usize,
    settings_show: bool,
    server_connected: ServerStatus,
    server_address: String,

    settings: settings::Handler,
    update_found: Option<String>,
    blacklist_info: blacklist::BlacklistResult,
}
impl Application for MainState {
    type Executor = executor::Default;
    type Flags = ();
    type Message = Message;
    type Theme = Theme;

    fn new(_: Self::Flags) -> (Self, Command<Self::Message>) {
        let mut new = Self::default();
        new.joycon = Some(joycon::Wrapper::new(new.settings.clone()));
        new.server_address = format!("{}", new.settings.load().get_socket_address());
        (
            new,
            Command::batch(vec![
                Command::perform(update::check_updates(), Message::UpdateFound),
                Command::perform(blacklist::check_blacklist(), Message::BlacklistChecked),
            ]),
        )
    }

    fn title(&self) -> String {
        "SlimeVR Wrangler".into()
    }
    fn theme(&self) -> Theme {
        Theme::Dark
    }

    fn update(&mut self, message: Message) -> Command<Self::Message> {
        match message {
            Message::SettingsPressed => {
                self.settings_show = !self.settings_show;
            }
            Message::Tick(_time) => {
                if let Some(ref ji) = self.joycon {
                    if let Some(res) = ji.poll_status() {
                        self.joycon_boxes.statuses = res;
                    }
                    if let Some(connected) = ji.poll_server() {
                        self.server_connected = connected;
                    }
                }
            }
            Message::Dot(_time) => {
                self.search_dots = (self.search_dots + 1) % 4;
            }
            Message::AddressChange(value) => {
                self.settings.change(|ws| ws.address = value);
            }
            Message::UpdateFound(version) => {
                self.update_found = version;
            }
            Message::UpdatePressed => {
                self.update_found = None;
                update::update();
            }
            Message::BlacklistChecked(info) => {
                self.blacklist_info = info;
            }
            Message::BlacklistFixPressed => {
                self.blacklist_info =
                    blacklist::BlacklistResult::info("Updating steam config file.....");
                return Command::perform(blacklist::update_blacklist(), Message::BlacklistChecked);
            }
            Message::JoyconRotate(serial_number, direction) => {
                self.settings.change(|ws| {
                    ws.joycon_rotation_add(serial_number, if direction { 90 } else { -90 });
                });
            }
            Message::JoyconScale(serial_number, scale) => {
                self.settings
                    .change(|ws| ws.joycon_scale_set(serial_number, scale));
            }
            Message::SettingsResetToggled(new) => {
                self.settings.change(|ws| ws.send_reset = new);
            }
            Message::SettingsIdsToggled(new) => {
                self.settings.change(|ws| ws.keep_ids = new);
            }
        }
        Command::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch(vec![
            time::every(Duration::from_millis(500)).map(Message::Dot),
            time::every(Duration::from_millis(50)).map(Message::Tick),
        ])
    }

    fn view(&self) -> Element<Message> {
        let mut app = Column::new().push(top_bar(self.update_found.clone()));

        if self.blacklist_info.visible() {
            app = app.push(blacklist_bar(&self.blacklist_info));
        }

        app.push(
            if self.settings_show {
                container(self.settings_screen()).padding(20)
            } else {
                container(self.joycon_screen())
            }
            .width(Length::Fill)
            .height(Length::Fill)
            .style(style::container_darker as for<'r> fn(&'r _) -> _),
        )
        .push(bottom_bar(
            self.server_connected,
            &".".repeat(self.search_dots),
            &self.server_address,
        ))
        .into()
    }
}

impl MainState {
    fn joycon_screen(&self) -> Scrollable<'_, Message> {
        let mut grid = Grid::with_column_width(320.0);
        for bax in self.joycon_boxes.view(&self.settings.load()) {
            grid.insert(container(bax).padding(10));
        }
        let list = Column::new().padding(10).width(Length::Fill).push(grid);

        let list = list.push(
            container(text(format!(
                "Searching for Joycon controllers{}\n\
                    Please pair controllers in your system's \
                    bluetooth settings if they don't show up here.",
                ".".repeat(self.search_dots)
            )))
            .padding(10),
        );
        scrollable(list).height(Length::Fill)
    }
    fn settings_screen(&self) -> Column<'_, Message> {
        Column::new()
            .spacing(20)
            .push(address(&self.settings.load().address))
            .push(checkbox(
                "Send yaw reset command to SlimeVR Server after B or UP button press.",
                self.settings.load().send_reset,
                Message::SettingsResetToggled,
            ))
            .push(checkbox(
                "Save mounting location on server. Requires SlimeVR Server v0.6.1 or newer. Restart Wrangler after changing this.",
                self.settings.load().keep_ids,
                Message::SettingsIdsToggled,
            ))
    }
}

fn address<'a>(input_value: &str) -> Column<'a, Message> {
    let address = text_input("127.0.0.1:6969", input_value)
        .on_input(Message::AddressChange)
        .width(Length::Fixed(300.0))
        .padding(10);

    let address_row = Row::new()
        .spacing(10)
        .align_items(Alignment::Center)
        .push("SlimeVR Server address:")
        .push(address)
        .push("Restart Wrangler after changing this.");
    let mut allc = Column::new().push(address_row).spacing(10);

    if input_value.parse::<SocketAddr>().is_err() {
        allc = allc.push(
            container(text(
                "Address is not a valid ip with port number! Using default instead (127.0.0.1:6969).",
            ))
            .style(style::text_yellow as for<'r> fn(&'r _) -> _),
        );
    }
    allc
}
fn top_bar<'a>(update: Option<String>) -> Container<'a, Message> {
    let mut top_column = Row::new()
        .align_items(Alignment::Center)
        .push(text("SlimeVR Wrangler").size(24));

    if let Some(u) = update {
        let update_btn = button(text("Update"))
            .style(theme::Button::Custom(Box::new(style::PrimaryButton)))
            .on_press(Message::UpdatePressed);
        top_column = top_column
            .push(horizontal_space(Length::Fixed(20.0)))
            .push(text(format!("New update found! Version: {u}. ")))
            .push(update_btn);
    }

    let settings = button(text("Settings"))
        .style(theme::Button::Custom(Box::new(style::PrimaryButton)))
        .on_press(Message::SettingsPressed);
    top_column = top_column
        .push(horizontal_space(Length::Fill))
        .push(settings);

    container(top_column)
        .width(Length::Fill)
        .padding(20)
        .style(style::container_highlight as for<'r> fn(&'r _) -> _)
}

fn blacklist_bar<'a>(result: &blacklist::BlacklistResult) -> Container<'a, Message> {
    let mut row = Row::new()
        .align_items(Alignment::Center)
        .push(text(result.info.clone()))
        .push(horizontal_space(Length::Fixed(20.0)));
    if result.fix_button {
        row = row.push(
            button(text("Fix blacklist"))
                .style(theme::Button::Custom(Box::new(style::PrimaryButton)))
                .on_press(Message::BlacklistFixPressed),
        );
    }
    container(row)
        .width(Length::Fill)
        .padding(20)
        .style(style::container_info as for<'r> fn(&'r _) -> _)
}

fn bottom_bar<'a>(
    connected: ServerStatus,
    search_dots: &String,
    address: &String,
) -> Container<'a, Message> {
    let status = Row::new()
        .push(text("Connection to SlimeVR Server: "))
        .push(container(text(format!("{connected:?}"))).style(
            if connected == ServerStatus::Connected {
                style::text_green
            } else {
                style::text_yellow
            },
        ))
        .push(text(if connected == ServerStatus::Connected {
            format!(" to {address}.")
        } else {
            format!(". Trying to connect to {address}{search_dots}")
        }));
    container(status)
        .width(Length::Fill)
        .padding(20)
        .style(style::container_info as for<'r> fn(&'r _) -> _)
}

#[derive(Debug)]
struct JoyconBoxes {
    pub statuses: Vec<joycon::Status>,
    svg_handler: joycon::Svg,
    needles: Vec<Needle>,
}

impl Default for JoyconBoxes {
    fn default() -> Self {
        Self {
            statuses: vec![],
            svg_handler: joycon::Svg::new(),
            needles: (0..360).map(Needle::new).collect(),
        }
    }
}

impl JoyconBoxes {
    fn view<'a>(&'a self, settings: &WranglerSettings) -> Vec<Container<'a, Message>> {
        self.statuses
            .iter()
            .map(|status| {
                container(single_box_view(
                    status,
                    &self.svg_handler,
                    &self.needles,
                    settings.joycon_scale_get(&status.serial_number),
                    settings.joycon_rotation_get(&status.serial_number),
                ))
                .height(Length::Fixed(335.0))
                .width(Length::Fixed(300.0))
                .padding(10)
                .style(style::item_normal as for<'r> fn(&'r _) -> _)
            })
            .collect()
    }
}

fn single_box_view<'a>(
    status: &joycon::Status,
    svg_handler: &joycon::Svg,
    needles: &'a [Needle],
    scale: f64,
    mount_rot: i32,
) -> Column<'a, Message> {
    let sn = status.serial_number.clone();

    let buttons = Row::new()
        .spacing(10)
        .push(
            button(text("↺").font(ICONS))
                .on_press(Message::JoyconRotate(sn.clone(), false))
                .style(theme::Button::Custom(Box::new(style::PrimaryButton))),
        )
        .push(
            button(text("↻").font(ICONS))
                .on_press(Message::JoyconRotate(sn.clone(), true))
                .style(theme::Button::Custom(Box::new(style::PrimaryButton))),
        );

    let svg = Svg::new(svg_handler.get(&status.design, mount_rot));

    let left = Column::new()
        .spacing(10)
        .align_items(Alignment::Center)
        .push(buttons)
        .push(svg)
        .width(Length::Fixed(130.0));

    let rot = status.rotation;
    let values = Row::with_children(
        [("Roll", rot.0), ("Pitch", rot.1), ("Yaw", -rot.2)]
            .iter()
            .map(|(name, val)| {
                let ival = (*val as i32).rem_euclid(360) as usize;
                let needle = needles.get(ival).unwrap_or_else(|| &needles[0]);

                Column::new()
                    .push(text(name))
                    .push(
                        canvas(needle)
                            .width(Length::Fixed(25.0))
                            .height(Length::Fixed(25.0)),
                    )
                    .push(text(format!("{ival}")))
                    .spacing(10)
                    .align_items(Alignment::Center)
                    .width(Length::Fill)
                    .into()
            })
            .collect(),
    );

    let circle = circle(
        8.0,
        match status.status {
            DeviceStatus::Disconnected | DeviceStatus::NoIMU => Color::from_rgb8(0xff, 0x38, 0x4A),
            DeviceStatus::LaggyIMU => Color::from_rgb8(0xff, 0xe3, 0x3c),
            DeviceStatus::Healthy => Color::from_rgb8(0x3d, 0xff, 0x81),
        },
    );

    let top = Row::new()
        .spacing(5)
        .push(circle)
        .push(left)
        .push(values)
        .height(Length::Fixed(150.0));

    let battery_text =
        container(text(format!("{:?}", status.battery))).style(match status.battery {
            Battery::Empty | Battery::Critical => style::text_orange,
            Battery::Low => style::text_yellow,
            Battery::Medium | Battery::Full => style::text_green,
        });

    let status_text = container(text(format!("{}", status.status))).style(match status.status {
        DeviceStatus::Disconnected | DeviceStatus::NoIMU => style::text_orange,
        DeviceStatus::LaggyIMU => style::text_yellow,
        DeviceStatus::Healthy => style::text_green,
    });

    let bottom = Column::new()
        .spacing(10)
        .push(
            slider(0.8..=1.2, scale, move |c| {
                Message::JoyconScale(sn.clone(), c)
            })
            .step(0.001),
        )
        .push(text(format!("Rotation scale ratio: {scale:.3}")))
        .push(
            text(
                "Change this if the tracker in VR moves less or more than your irl Joycon. Higher value = more movement.",
            )
            .size(14),
        )
        .push(Row::new().push(text("Battery level: ")).push(battery_text))
        .push(Row::new().push(text("Status: ")).push(status_text));

    Column::new().spacing(10).push(top).push(bottom)
}
