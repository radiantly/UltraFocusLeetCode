#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod hooks;

use crossbeam_channel::{Receiver, Sender, unbounded};
use eframe::{
    self, Renderer,
    egui::{self, FontData, FontDefinitions, FontFamily, FontId, Margin, RichText, TextStyle},
};
use is_elevated::is_elevated;
use std::mem;
use std::sync::Arc;
use std::thread;
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, RECT, TRUE, WPARAM},
        Graphics::Gdi::{
            GetMonitorInfoW, MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromWindow,
        },
        UI::{
            Input::KeyboardAndMouse::{
                INPUT, INPUT_0, INPUT_KEYBOARD, KEYEVENTF_KEYUP, SendInput, VK_F11,
            },
            WindowsAndMessaging::{
                EnumWindows, GetWindowRect, GetWindowTextW, IsWindowVisible, PostMessageW,
                SC_MINIMIZE, SetForegroundWindow, WM_SYSCOMMAND,
            },
        },
    },
    core::BOOL,
};

struct WindowInfo {
    hwnd: HWND,
    title: String,
}

unsafe extern "system" fn enum_windows_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let is_visible = unsafe { IsWindowVisible(hwnd) };
    if !is_visible.as_bool() {
        return TRUE;
    }

    // get title
    let mut buf = [0u16; 128];
    let title_len = unsafe { GetWindowTextW(hwnd, &mut buf) };
    if title_len == 0 {
        return TRUE;
    }

    let title = String::from_utf16_lossy(&buf[..title_len as usize]);

    let out: &mut Vec<WindowInfo> = unsafe { &mut *(lparam.0 as *mut _) };
    out.push(WindowInfo { hwnd, title });

    TRUE // continue enumeration
}

fn get_leetcode_window() -> Vec<WindowInfo> {
    let mut hwnd = Vec::new();

    unsafe {
        let param = LPARAM(&mut hwnd as *mut _ as isize);
        let _ = EnumWindows(Some(enum_windows_proc), param);
    }
    hwnd
}

fn is_full_screen(hwnd: HWND) -> bool {
    let mut monitor_info = MONITORINFO::default();
    monitor_info.cbSize = mem::size_of::<MONITORINFO>() as u32;
    let monitor_result = unsafe {
        GetMonitorInfoW(
            MonitorFromWindow(hwnd, MONITOR_DEFAULTTOPRIMARY),
            &mut monitor_info as *mut _,
        )
    };
    if !monitor_result.as_bool() {
        return true;
    }

    let mut window_rect = RECT::default();
    if unsafe { GetWindowRect(hwnd, &mut window_rect as *mut _) }.is_err() {
        return true;
    }
    window_rect.left == monitor_info.rcMonitor.left
        && window_rect.right == monitor_info.rcMonitor.right
        && window_rect.top == monitor_info.rcMonitor.top
        && window_rect.bottom == monitor_info.rcMonitor.bottom
}

enum WorkerResponse {
    SUCCESS(String),
    ERROR(String),
}

unsafe impl Send for WorkerResponse {}

struct Gui {
    is_elevated: bool,
    last_error: Option<String>,
    worker_send: Sender<bool>,
    worker_recv: Receiver<WorkerResponse>,
}

impl Gui {
    fn new() -> Gui {
        let (worker_send, main_recv) = unbounded();
        let (main_send, worker_recv) = unbounded();

        thread::spawn(move || {
            for _ in main_recv.iter() {
                println!("attempting to get window");
                let windows = get_leetcode_window();
                let lc_window = windows.iter().find(|window| {
                    window.title != "UltraFocusLeetCode" && window.title.contains("LeetCode")
                });
                if let Some(window) = lc_window {
                    let _ = main_send.send(WorkerResponse::SUCCESS(format!(
                        "Focusing {}",
                        window.title
                    )));
                } else {
                    let _ = main_send.send(WorkerResponse::ERROR(String::from(
                        "Could not find a window with LeetCode in the title",
                    )));
                    continue;
                }

                let lc_window = lc_window.unwrap();

                for window in windows.iter() {
                    if window.title == "UltraFocusLeetCode" {
                        continue;
                    }
                    if window.hwnd == lc_window.hwnd {
                        continue;
                    }
                    let _ = unsafe {
                        PostMessageW(
                            Some(window.hwnd),
                            WM_SYSCOMMAND,
                            WPARAM(SC_MINIMIZE as usize),
                            LPARAM::default(),
                        )
                    };
                }

                let _ = unsafe { SetForegroundWindow(lc_window.hwnd) };

                // if not full screen, maximize the window using F11
                if !is_full_screen(lc_window.hwnd) {
                    let mut f11_input = INPUT_0::default();
                    f11_input.ki.wVk = VK_F11;

                    let mut f11_input_up = f11_input.clone();
                    f11_input_up.ki.dwFlags = KEYEVENTF_KEYUP;

                    let inputs = [
                        INPUT {
                            r#type: INPUT_KEYBOARD,
                            Anonymous: f11_input,
                        },
                        INPUT {
                            r#type: INPUT_KEYBOARD,
                            Anonymous: f11_input_up,
                        },
                    ];

                    unsafe { SendInput(&inputs, mem::size_of::<INPUT>() as i32) };
                }
                let hwnd_u32 = lc_window.hwnd.0 as u32;

                thread::spawn(move || {
                    if let Err(err) = hooks::hook(hwnd_u32) {
                        eprintln!("{:?}", err);
                    }
                });
            }
        });

        Gui {
            is_elevated: is_elevated(),
            last_error: None,
            worker_send,
            worker_recv,
        }
    }

    fn nice_label(ui: &mut egui::Ui, text: &str) {
        ui.label(RichText::new(text).line_height(Some(18.0)));
    }
}

impl eframe::App for Gui {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // graceful close
        if ctx.input(|i| i.viewport().close_requested()) {
            return;
        }

        egui::CentralPanel::default()
        .frame(egui::Frame::central_panel(&ctx.style()).inner_margin(Margin::same(14)))
        .show(ctx, |ui| {
                // if !self.is_elevated {
                //     Self::nice_label(ui, "Please run as administrator.");
                //     return;
                // }
                Self::nice_label(ui, "To start the ultra focus mode, navigate to a leetcode problem on your browser and then click the button below:");
                ui.add_space(10.0);
                if ui.add(egui::Button::new(
                    RichText::new("Start LeetCode Ultra Focus").text_style(TextStyle::Heading).size(24.0),
                )).clicked() {
                    let _ = self.worker_send.send(true);
                }

                if let Ok(event) = self.worker_recv.try_recv() {
                    self.last_error = match event {
                        WorkerResponse::SUCCESS(success) => Some(success),
                        WorkerResponse::ERROR(err) => Some(err)
                    }
                }

                if let Some(err) = &self.last_error {
                    ui.add_space(10.0);
                    Self::nice_label(ui, err);
                }
            });
    }
}

fn main() {
    let mut options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([380.0, 180.0]),
        renderer: Renderer::Wgpu,
        ..Default::default()
    };

    eframe::run_native(
        "UltraFocusLeetCode",
        options,
        Box::new(|cc| {
            // This gives us image support:
            // egui_extras::install_image_loaders(&cc.egui_ctx);

            let mut fonts = FontDefinitions::default();

            fonts.font_data.insert(
                "Inter_18pt-Regular".to_owned(),
                Arc::new(FontData::from_static(include_bytes!(
                    "../assets/fonts/Inter_18pt-Regular.ttf"
                ))),
            );

            fonts.families.insert(
                FontFamily::Name("Inter_18pt-Regular".into()),
                vec!["Inter_18pt-Regular".to_owned()],
            );

            fonts.font_data.insert(
                "Inter_18pt-Bold".to_owned(),
                Arc::new(FontData::from_static(include_bytes!(
                    "../assets/fonts/Inter_18pt-Bold.ttf"
                ))),
            );

            fonts.families.insert(
                FontFamily::Name("Inter_18pt-Bold".into()),
                vec!["Inter_18pt-Bold".to_owned()],
            );

            cc.egui_ctx.set_fonts(fonts);

            cc.egui_ctx.all_styles_mut(|style| {
                // do not allow text to be selected
                style.interaction.selectable_labels = false;

                // button padding
                style.spacing.button_padding = egui::vec2(15.0, 5.0);

                let mut text_styles = style.text_styles.clone();
                text_styles.insert(
                    TextStyle::Body,
                    FontId {
                        size: 12.0,
                        family: egui::FontFamily::Name("Inter_18pt-Regular".into()),
                    },
                );

                text_styles.insert(
                    TextStyle::Heading,
                    FontId {
                        size: 16.0,
                        family: egui::FontFamily::Name("Inter_18pt-Bold".into()),
                    },
                );
                style.text_styles = text_styles;
            });

            Ok(Box::new(Gui::new()))
        }),
    )
    .expect("Failed to create window");
}
