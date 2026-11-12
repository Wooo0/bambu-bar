// 窗口与托盘：弹出面板的创建、定位、隐藏；设置窗按需创建。

use crate::state::AppState;
use std::sync::Arc;
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent,
};

pub const POPOVER_W: f64 = 400.0;
pub const POPOVER_H: f64 = 420.0;

pub fn create_tray(app: &AppHandle, state: Arc<AppState>) -> tauri::Result<()> {
    let refresh = MenuItemBuilder::with_id("refresh", "刷新全部").build(app)?;
    let settings = MenuItemBuilder::with_id("settings", "设置…").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "退出 Bambu Bar").build(app)?;
    let menu = MenuBuilder::new(app)
        .items(&[&refresh, &settings])
        .separator()
        .item(&quit)
        .build()?;

    let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))
        .expect("tray icon");

    let mut builder = TrayIconBuilder::with_id("main")
        .icon(icon)
        .icon_as_template(true)
        .tooltip("Bambu Bar")
        .menu(&menu)
        .show_menu_on_left_click(false);

    #[cfg(target_os = "macos")]
    {
        // 模板图：跟随系统明暗自动反色，无需准备两套图标。
        builder = builder.icon_as_template(true);
    }

    builder
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "refresh" => {
                let st = app.state::<Arc<AppState>>().inner().clone();
                for item in st.tray_items() {
                    st.request_pushall(&item.serial);
                }
            }
            // show_settings 内部会把自己排队到主线程事件循环的下一拍，
            // 这里直接调用不会阻塞菜单事件回调。
            "settings" => show_settings(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(move |tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                toggle_popover(tray.app_handle());
            }
        })
        .build(app)?;

    let _ = state;
    Ok(())
}

pub fn create_popover(app: &AppHandle) -> tauri::Result<()> {
    let win = WebviewWindowBuilder::new(app, "popover", WebviewUrl::App("index.html".into()))
        .title("Bambu Bar")
        .inner_size(POPOVER_W, POPOVER_H)
        .resizable(false)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .focused(false)
        .shadow(true)
        .on_page_load(|_w, payload| {
            // 只在调试开关下打：页面到底有没有加载成功，是排查「面板空白 / 不测量高度」
            // 的第一现场——没有这行，失败在 WebKit 控制台里完全看不见。
            if std::env::var_os("BAMBU_BAR_DEBUG_TRAY").is_some() {
                eprintln!("[webview] {:?} {}", payload.event(), payload.url());
            }
        })
        .build()?;

    // 失焦即收起：菜单栏面板的通行行为（点别处等于关闭）。
    let w = win.clone();
    win.on_window_event(move |event| {
        if let WindowEvent::Focused(false) = event {
            let _ = w.hide();
        }
    });
    Ok(())
}

pub fn toggle_popover(app: &AppHandle) {
    let Some(win) = app.get_webview_window("popover") else {
        eprintln!("[panel] popover 窗口不存在");
        return;
    };
    if win.is_visible().unwrap_or(false) {
        let _ = win.hide();
        return;
    }
    let h = win
        .outer_size()
        .map(|s| s.to_logical::<f64>(win.scale_factor().unwrap_or(1.0)).height)
        .unwrap_or(POPOVER_H);
    position_near_tray(app, &win, h);
    let _ = win.show();
    let _ = win.set_focus();
}

/// 把面板贴到托盘图标旁边，并保证完整落在可见区域内。
/// 锚点是托盘矩形本身：
/// - 托盘在上半屏（macOS 菜单栏）→ 面板悬在托盘下方；
/// - 托盘在下半屏（Windows 任务栏）→ 面板整体悬在托盘上方。
/// panel_h 由调用方传入（set_panel_height 刚 set_size 出去的值在
/// outer_size 里可能还没生效，以参数为准而不是读回旧值）。
pub fn position_near_tray(app: &AppHandle, win: &tauri::WebviewWindow, panel_h: f64) {
    let panel_h = if panel_h > 0.0 { panel_h } else { POPOVER_H };
    let win_scale = win.scale_factor().unwrap_or(1.0);

    // 锚点：优先托盘矩形（逻辑坐标），拿不到退化为鼠标光标处的伪矩形
    let anchor = app
        .tray_by_id("main")
        .and_then(|tray| tray.rect().ok().flatten())
        .map(|rect| {
            let pos = rect.position.to_logical::<f64>(win_scale);
            let size = rect.size.to_logical::<f64>(win_scale);
            (pos.x, pos.y, size.width, size.height)
        })
        .or_else(|| {
            app.cursor_position().ok().map(|p| {
                let p = p.to_logical::<f64>(win_scale);
                (p.x - 8.0, p.y, 16.0, 2.0)
            })
        });
    let Some((tx, ty, tw, th)) = anchor else { return };

    let mut px = tx + tw / 2.0 - POPOVER_W / 2.0;
    let mut py;

    if let Ok(Some(monitor)) = win.current_monitor() {
        let msf = monitor.scale_factor();
        let msize = monitor.size().to_logical::<f64>(msf);
        let mpos = monitor.position().to_logical::<f64>(msf);

        let tray_center_y = ty + th / 2.0;
        if tray_center_y < mpos.y + msize.height / 2.0 {
            py = ty + th + 6.0; // 上半屏：菜单栏下方
        } else {
            py = ty - panel_h - 6.0; // 下半屏：任务栏上方
        }
        // 兜底收敛，正常情况上面已经算在屏幕内
        px = px.clamp(mpos.x + 8.0, mpos.x + msize.width - POPOVER_W - 8.0);
        py = py.clamp(mpos.y + 8.0, mpos.y + msize.height - panel_h - 8.0);
    } else {
        py = ty + th + 6.0;
    }

    let _ = win.set_position(tauri::LogicalPosition::new(px, py));
}

/// 打开设置窗。所有调用点（托盘菜单 / 面板按钮 / --settings 启动参数）
/// 统一走这里，线程安全：
/// - Windows 上窗口必须在主线程创建，挂到别的线程上 webview 永远加载
///   不出来（白屏），还会卡死焦点事件让面板失去「失焦即隐藏」；
/// - 但也不能在托盘菜单/命令的回调栈里同步创建——那会让事件循环等
///   自己，直接死锁（关都关不掉）。
/// run_on_main_thread 把创建动作排队到事件循环下一拍：既在主线程，
/// 又不在当前回调栈里。
pub fn show_settings(app: &AppHandle) {
    let h = app.clone();
    let _ = h.run_on_main_thread(move || {
        if let Some(w) = h.get_webview_window("settings") {
            let _ = w.show();
            let _ = w.unminimize();
            let _ = w.set_focus();
            return;
        }
        let _ = WebviewWindowBuilder::new(&h, "settings", WebviewUrl::App("settings.html".into()))
            .title("Bambu Bar 设置")
            .inner_size(580.0, 840.0)
            .min_inner_size(480.0, 560.0)
            .resizable(true)
            .build();
    });
}
