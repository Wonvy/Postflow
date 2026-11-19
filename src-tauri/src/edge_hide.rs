//! 靠边隐藏：把窗口拖到屏幕的上、左、右边缘，鼠标离开后窗口缩进屏幕外，只露出一条细边；
//! 鼠标碰到那条边，窗口滑回来。设置里打开（默认关），只在这台设备上。
//!
//! 一个后台线程每 80 毫秒看一次窗口位置和鼠标位置。只认屏幕真正的边缘：两块屏幕相接的那条边不算，
//! 不然窗口会缩到另一块屏幕上。底边不认（任务栏、程序坞在那里）。最大化、全屏、最小化时不管。

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager, PhysicalPosition, Runtime, WebviewWindow};

/// 藏起来时露在屏幕里的宽度（逻辑像素）。
const STRIP: f64 = 4.0;
/// 窗口离屏幕边缘这么近（逻辑像素）就算贴边。
const SNAP: f64 = 8.0;
/// 鼠标离开窗口这么久才藏，免得划过去一下就藏了。
const LEAVE_DELAY: Duration = Duration::from_millis(500);
/// 鼠标在窗口外这么远（逻辑像素）以内还算在窗口上。
const MARGIN: f64 = 6.0;
const TICK: Duration = Duration::from_millis(80);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Edge {
    Top,
    Left,
    Right,
}

/// 一块屏幕的可用区域（去掉菜单栏、任务栏），物理像素。
#[derive(Debug, Clone, Copy, PartialEq)]
struct Area {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

impl Area {
    fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Rect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum State {
    /// 没贴边。
    Free,
    /// 贴着边、露在外面。`entered`：鼠标进过窗口（拖过来的、或者从边上滑出来以后）；
    /// 没进过的（比如从托盘打开）不藏，免得一打开就缩回去。
    Shown {
        edge: Edge,
        area: Area,
        scale: f64,
        entered: bool,
        left_at: Option<Instant>,
    },
    /// 缩在屏幕外。`at`：藏起来时窗口的位置，用户自己把它拖走了就不再管。
    Hidden {
        edge: Edge,
        area: Area,
        scale: f64,
        at: (i32, i32),
    },
}

pub struct EdgeHide {
    enabled: AtomicBool,
    state: Mutex<State>,
}

impl Default for EdgeHide {
    fn default() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            state: Mutex::new(State::Free),
        }
    }
}

/// 贴的是哪条边。只认屏幕真正的边缘：边外面一点的地方不在任何屏幕上。
fn docked_edge(win: Rect, area: Area, scale: f64, screens: &[Area]) -> Option<Edge> {
    let snap = SNAP * scale;
    let outside = |x: f64, y: f64| !screens.iter().any(|s| s.contains(x, y));
    let cx = win.x + win.w / 2.0;
    let cy = win.y + win.h / 2.0;
    if win.y <= area.y + snap && outside(cx, area.y - 2.0 * snap - 40.0 * scale) {
        return Some(Edge::Top);
    }
    if win.x <= area.x + snap && outside(area.x - 2.0 * snap, cy) {
        return Some(Edge::Left);
    }
    if win.x + win.w >= area.x + area.w - snap && outside(area.x + area.w + 2.0 * snap, cy) {
        return Some(Edge::Right);
    }
    None
}

/// 贴边露出来时窗口的位置（只改贴着的那个方向）。
fn shown_at(edge: Edge, win: Rect, area: Area) -> (f64, f64) {
    match edge {
        Edge::Top => (win.x, area.y),
        Edge::Left => (area.x, win.y),
        Edge::Right => (area.x + area.w - win.w, win.y),
    }
}

/// 藏起来时窗口的位置：只露出 STRIP 宽的一条。
fn hidden_at(edge: Edge, win: Rect, area: Area, scale: f64) -> (f64, f64) {
    let strip = STRIP * scale;
    match edge {
        Edge::Top => (win.x, area.y - win.h + strip),
        Edge::Left => (area.x - win.w + strip, win.y),
        Edge::Right => (area.x + area.w - strip, win.y),
    }
}

/// 鼠标碰到了藏起来的窗口露出的那条边（或者那条边外面的屏幕边缘）。
fn touches_strip(edge: Edge, win: Rect, area: Area, scale: f64, cx: f64, cy: f64) -> bool {
    let strip = STRIP * scale;
    match edge {
        Edge::Top => cx >= win.x && cx <= win.x + win.w && cy <= area.y + strip,
        Edge::Left => cy >= win.y && cy <= win.y + win.h && cx <= area.x + strip,
        Edge::Right => cy >= win.y && cy <= win.y + win.h && cx >= area.x + area.w - strip,
    }
}

fn over_window(win: Rect, scale: f64, cx: f64, cy: f64) -> bool {
    let m = MARGIN * scale;
    cx >= win.x - m && cx <= win.x + win.w + m && cy >= win.y - m && cy <= win.y + win.h + m
}

fn screens<R: Runtime>(window: &WebviewWindow<R>) -> Vec<(Area, f64)> {
    window
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| {
            let a = m.work_area();
            (
                Area {
                    x: a.position.x as f64,
                    y: a.position.y as f64,
                    w: a.size.width as f64,
                    h: a.size.height as f64,
                },
                m.scale_factor(),
            )
        })
        .collect()
}

fn rect<R: Runtime>(window: &WebviewWindow<R>) -> Option<Rect> {
    let p = window.outer_position().ok()?;
    let s = window.outer_size().ok()?;
    Some(Rect {
        x: p.x as f64,
        y: p.y as f64,
        w: s.width as f64,
        h: s.height as f64,
    })
}

fn move_to<R: Runtime>(window: &WebviewWindow<R>, (x, y): (f64, f64)) -> (i32, i32) {
    let at = (x.round() as i32, y.round() as i32);
    let _ = window.set_position(PhysicalPosition::new(at.0, at.1));
    at
}

/// 设置里开关（界面通过 tray_update 告诉这里）。关掉时藏着的窗口先拿出来。
pub fn set_enabled<R: Runtime>(app: &AppHandle<R>, enabled: bool) {
    let edge = app.state::<EdgeHide>();
    edge.enabled.store(enabled, Ordering::Relaxed);
    if !enabled {
        reveal(app);
        *edge.state.lock().unwrap() = State::Free;
    }
}

/// 窗口藏着的话拿出来（从托盘、程序坞、通知打开时）。拿出来以后鼠标进过窗口、再离开才会再藏。
pub fn reveal<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let edge_hide = app.state::<EdgeHide>();
    let mut state = edge_hide.state.lock().unwrap();
    if let State::Hidden {
        edge, area, scale, ..
    } = *state
        && let Some(win) = rect(&window)
    {
        move_to(&window, shown_at(edge, win, area));
        *state = State::Shown {
            edge,
            area,
            scale,
            entered: false,
            left_at: None,
        };
    }
}

/// 应用启动时开始看着窗口。
pub fn start<R: Runtime>(app: &AppHandle<R>) {
    let app = app.clone();
    std::thread::spawn(move || {
        let mut last: Option<Rect> = None;
        let mut was_focused = false;
        loop {
            std::thread::sleep(TICK);
            let edge_hide = app.state::<EdgeHide>();
            if !edge_hide.enabled.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(400));
                continue;
            }
            let Some(window) = app.get_webview_window("main") else {
                continue;
            };
            let usable = window.is_visible().unwrap_or(false)
                && !window.is_minimized().unwrap_or(false)
                && !window.is_maximized().unwrap_or(false)
                && !window.is_fullscreen().unwrap_or(false);
            let (Some(win), Ok(cursor)) = (rect(&window), window.cursor_position()) else {
                continue;
            };
            let focused = window.is_focused().unwrap_or(false);
            let moving = last.is_some_and(|l| l != win);
            last = Some(win);
            let mut state = edge_hide.state.lock().unwrap();
            if !usable {
                if !matches!(*state, State::Hidden { .. }) {
                    *state = State::Free;
                }
                was_focused = focused;
                continue;
            }
            let (cx, cy) = (cursor.x, cursor.y);
            match *state {
                State::Hidden {
                    edge,
                    area,
                    scale,
                    at,
                } => {
                    if (win.x.round() as i32, win.y.round() as i32) != at {
                        // 用户自己把窗口拖出来了，或者别的程序动了它
                        *state = State::Free;
                    } else {
                        let touched = touches_strip(edge, win, area, scale, cx, cy);
                        // 鼠标碰到边，或者用 ⌘Tab / Alt+Tab 切回来
                        if touched || (focused && !was_focused) {
                            move_to(&window, shown_at(edge, win, area));
                            tracing::debug!(?edge, touched, "edge hide: shown");
                            *state = State::Shown {
                                edge,
                                area,
                                scale,
                                // 切回来的：等鼠标进过窗口再说
                                entered: touched,
                                left_at: None,
                            };
                        }
                    }
                }
                State::Shown {
                    edge,
                    area,
                    scale,
                    entered,
                    left_at,
                } => {
                    let all = screens(&window);
                    let areas: Vec<Area> = all.iter().map(|(a, _)| *a).collect();
                    if docked_edge(win, area, scale, &areas) != Some(edge) {
                        *state = State::Free;
                    } else if over_window(win, scale, cx, cy) || moving {
                        *state = State::Shown {
                            edge,
                            area,
                            scale,
                            entered: entered || over_window(win, scale, cx, cy),
                            left_at: None,
                        };
                    } else if !entered {
                        // 还没进过窗口：等着
                    } else if let Some(since) = left_at {
                        if since.elapsed() >= LEAVE_DELAY {
                            // 先对齐到边上再藏，藏的位置才准
                            let (sx, sy) = shown_at(edge, win, area);
                            let aligned = Rect {
                                x: sx,
                                y: sy,
                                ..win
                            };
                            let at = move_to(&window, hidden_at(edge, aligned, area, scale));
                            tracing::debug!(?edge, x = at.0, y = at.1, "edge hide: hidden");
                            last = None;
                            *state = State::Hidden {
                                edge,
                                area,
                                scale,
                                at,
                            };
                        }
                    } else {
                        *state = State::Shown {
                            edge,
                            area,
                            scale,
                            entered,
                            left_at: Some(Instant::now()),
                        };
                    }
                }
                State::Free => {
                    if !moving {
                        let all = screens(&window);
                        let areas: Vec<Area> = all.iter().map(|(a, _)| *a).collect();
                        // 窗口所在的那块屏幕：窗口中心（或最近的）在哪块上
                        let (wx, wy) = (win.x + win.w / 2.0, win.y.max(0.0) + 1.0);
                        let here = all
                            .iter()
                            .find(|(a, _)| a.contains(wx, wy))
                            .or_else(|| {
                                all.iter()
                                    .find(|(a, _)| a.contains(wx, win.y + win.h / 2.0))
                            })
                            .copied();
                        if let Some((area, scale)) = here
                            && let Some(edge) = docked_edge(win, area, scale, &areas)
                        {
                            tracing::debug!(?edge, "edge hide: docked");
                            *state = State::Shown {
                                edge,
                                area,
                                scale,
                                entered: over_window(win, scale, cx, cy),
                                left_at: None,
                            };
                        }
                    }
                }
            }
            was_focused = focused;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEFT: Area = Area {
        x: 0.0,
        y: 50.0,
        w: 2880.0,
        h: 1700.0,
    };
    const RIGHT: Area = Area {
        x: 2880.0,
        y: 0.0,
        w: 1920.0,
        h: 1040.0,
    };

    fn win(x: f64, y: f64) -> Rect {
        Rect {
            x,
            y,
            w: 1000.0,
            h: 700.0,
        }
    }

    #[test]
    fn only_real_screen_edges_count() {
        let both = [LEFT, RIGHT];
        assert_eq!(
            docked_edge(win(500.0, 52.0), LEFT, 2.0, &both),
            Some(Edge::Top)
        );
        assert_eq!(
            docked_edge(win(4.0, 400.0), LEFT, 2.0, &both),
            Some(Edge::Left)
        );
        // 左边屏幕的右边缘挨着右边那块屏幕，不算
        assert_eq!(docked_edge(win(1880.0, 400.0), LEFT, 2.0, &both), None);
        // 只有一块屏幕时就算
        assert_eq!(
            docked_edge(win(1880.0, 400.0), LEFT, 2.0, &[LEFT]),
            Some(Edge::Right)
        );
        assert_eq!(docked_edge(win(500.0, 400.0), LEFT, 2.0, &both), None);
    }

    #[test]
    fn hidden_window_keeps_a_strip_on_screen() {
        let w = win(500.0, 50.0);
        assert_eq!(
            hidden_at(Edge::Top, w, LEFT, 2.0),
            (500.0, 50.0 - 700.0 + 8.0)
        );
        assert_eq!(hidden_at(Edge::Left, w, LEFT, 2.0), (-1000.0 + 8.0, 50.0));
        assert_eq!(hidden_at(Edge::Right, w, LEFT, 2.0), (2880.0 - 8.0, 50.0));
        assert_eq!(
            shown_at(Edge::Right, win(2000.0, 300.0), LEFT),
            (1880.0, 300.0)
        );
    }

    #[test]
    fn touching_the_strip_reveals() {
        let hidden = win(500.0, -642.0);
        assert!(touches_strip(Edge::Top, hidden, LEFT, 2.0, 800.0, 52.0));
        assert!(!touches_strip(Edge::Top, hidden, LEFT, 2.0, 1600.0, 52.0));
        assert!(!touches_strip(Edge::Top, hidden, LEFT, 2.0, 800.0, 200.0));
    }
}
