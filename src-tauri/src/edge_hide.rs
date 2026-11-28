//! 靠边隐藏：把窗口拖到屏幕的上、左、右边缘，鼠标离开后窗口缩进屏幕外，只露出一条细边；
//! 鼠标碰到那条边，窗口滑回来。设置里打开（默认关），只在这台设备上。
//!
//! 一个后台线程每 80 毫秒看一次窗口位置和鼠标位置。只认屏幕真正的边缘：两块屏幕相接的那条边不算，
//! 不然窗口会缩到另一块屏幕上。底边不认（任务栏、程序坞在那里）。最大化、全屏、最小化时不管。
//!
//! 锁的规矩：后台线程拿着 `state` 的锁时绝不碰窗口。后台线程问窗口的位置、屏幕这些，要主线程来回答；
//! 主线程上的托盘点击、`tray_update` 又要拿这把锁。两边各等各的，整个应用就卡死了（托盘点了打不开窗口、
//! 靠边也不动）。所以先在锁外面把窗口的样子读好（`Seen`），拿锁只算该怎么办（`step`），
//! 要挪窗口就交给主线程去挪（主线程拿着锁碰窗口没事，它不用等别人）。
//!
//! 挪之前主线程再看一眼状态还是不是后台线程算出来的那个：后台线程算出「该藏了」到真的挪之间，
//! 用户可能刚点了托盘图标把窗口拿出来，这时不能再把它挪到屏幕外面去。
//!
//! Windows 的「贴靠」会把拖到屏幕顶上的窗口最大化。开着靠边隐藏时把它还原，当成贴到了顶边。

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager, PhysicalPosition, Runtime, Window};

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

fn screens<R: Runtime>(window: &Window<R>) -> Vec<(Area, f64)> {
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

fn rect<R: Runtime>(window: &Window<R>) -> Option<Rect> {
    let p = window.outer_position().ok()?;
    let s = window.outer_size().ok()?;
    Some(Rect {
        x: p.x as f64,
        y: p.y as f64,
        w: s.width as f64,
        h: s.height as f64,
    })
}

fn move_to<R: Runtime>(window: &Window<R>, (x, y): (f64, f64)) -> (i32, i32) {
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
        *edge.state.lock().unwrap_or_else(|e| e.into_inner()) = State::Free;
    }
}

/// 窗口藏着的话拿出来（从托盘、程序坞、通知打开时）。拿出来以后鼠标进过窗口、再离开才会再藏。
/// 正贴着边、鼠标刚离开、过一会儿就要藏的，也不藏了：用户刚从托盘把它叫出来，鼠标还在托盘那边。
pub fn reveal<R: Runtime>(app: &AppHandle<R>) {
    // 必须用 get_window：原网页模式加上子网页视图后 get_webview_window("main") 会返回 None
    let Some(window) = app.get_window("main") else {
        return;
    };
    // 锁里只改状态；碰窗口放到锁外面（见模块说明）
    let hidden = {
        let edge_hide = app.state::<EdgeHide>();
        let mut state = edge_hide.state.lock().unwrap_or_else(|e| e.into_inner());
        match *state {
            State::Hidden {
                edge, area, scale, ..
            } => {
                *state = State::Shown {
                    edge,
                    area,
                    scale,
                    entered: false,
                    left_at: None,
                };
                Some((edge, area))
            }
            State::Shown {
                edge, area, scale, ..
            } => {
                *state = State::Shown {
                    edge,
                    area,
                    scale,
                    entered: false,
                    left_at: None,
                };
                None
            }
            State::Free => None,
        }
    };
    if let Some((edge, area)) = hidden
        && let Some(win) = rect(&window)
    {
        move_to(&window, shown_at(edge, win, area));
    }
}

/// 一次看到的窗口和鼠标。在锁外面读好。
#[derive(Debug, Clone)]
struct Seen {
    win: Rect,
    cursor: (f64, f64),
    focused: bool,
    /// 看得见、没最小化、没最大化、没全屏：这时才管。
    usable: bool,
    maximized: bool,
    screens: Vec<(Area, f64)>,
}

fn observe<R: Runtime>(window: &Window<R>) -> Option<Seen> {
    let maximized = window.is_maximized().unwrap_or(false);
    let usable = window.is_visible().unwrap_or(false)
        && !window.is_minimized().unwrap_or(false)
        && !maximized
        && !window.is_fullscreen().unwrap_or(false);
    let win = rect(window)?;
    let cursor = window.cursor_position().ok()?;
    Some(Seen {
        win,
        cursor: (cursor.x, cursor.y),
        focused: window.is_focused().unwrap_or(false),
        usable,
        maximized,
        screens: screens(window),
    })
}

/// 窗口所在的那块屏幕：窗口顶边中间（或者中心）在哪块上。
fn screen_of(win: Rect, screens: &[(Area, f64)]) -> Option<(Area, f64)> {
    let wx = win.x + win.w / 2.0;
    screens
        .iter()
        .find(|(a, _)| a.contains(wx, win.y.max(0.0) + 1.0))
        .or_else(|| {
            screens
                .iter()
                .find(|(a, _)| a.contains(wx, win.y + win.h / 2.0))
        })
        .copied()
}

/// 看了这一眼以后该怎么办：改 `state`，要挪窗口就返回挪到哪儿。只算，不碰窗口。
fn step(
    state: &mut State,
    seen: &Seen,
    moving: bool,
    was_focused: bool,
    now: Instant,
) -> Option<(i32, i32)> {
    let win = seen.win;
    let (cx, cy) = seen.cursor;
    if !seen.usable {
        if !matches!(*state, State::Hidden { .. }) {
            *state = State::Free;
        }
        return None;
    }
    let areas: Vec<Area> = seen.screens.iter().map(|(a, _)| *a).collect();
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
                return None;
            }
            let touched = touches_strip(edge, win, area, scale, cx, cy);
            // 鼠标碰到边，或者用 ⌘Tab / Alt+Tab 切回来
            if touched || (seen.focused && !was_focused) {
                *state = State::Shown {
                    edge,
                    area,
                    scale,
                    // 切回来的：等鼠标进过窗口再说
                    entered: touched,
                    left_at: None,
                };
                return Some(round(shown_at(edge, win, area)));
            }
            None
        }
        State::Shown {
            edge,
            area,
            scale,
            entered,
            left_at,
        } => {
            let over = over_window(win, scale, cx, cy);
            if docked_edge(win, area, scale, &areas) != Some(edge) {
                *state = State::Free;
            } else if over || moving {
                *state = State::Shown {
                    edge,
                    area,
                    scale,
                    entered: entered || over,
                    left_at: None,
                };
            } else if !entered {
                // 还没进过窗口：等着
            } else if let Some(since) = left_at {
                if now.duration_since(since) >= LEAVE_DELAY {
                    // 先对齐到边上再藏，藏的位置才准
                    let (sx, sy) = shown_at(edge, win, area);
                    let aligned = Rect {
                        x: sx,
                        y: sy,
                        ..win
                    };
                    let at = round(hidden_at(edge, aligned, area, scale));
                    *state = State::Hidden {
                        edge,
                        area,
                        scale,
                        at,
                    };
                    return Some(at);
                }
            } else {
                *state = State::Shown {
                    edge,
                    area,
                    scale,
                    entered,
                    left_at: Some(now),
                };
            }
            None
        }
        State::Free => {
            if moving {
                return None;
            }
            if let Some((area, scale)) = screen_of(win, &seen.screens)
                && let Some(edge) = docked_edge(win, area, scale, &areas)
            {
                *state = State::Shown {
                    edge,
                    area,
                    scale,
                    entered: over_window(win, scale, cx, cy),
                    left_at: None,
                };
            }
            None
        }
    }
}

fn round((x, y): (f64, f64)) -> (i32, i32) {
    (x.round() as i32, y.round() as i32)
}

/// Windows 把拖到屏幕顶上的窗口最大化了（「贴靠」）：上一眼还没最大化、窗口顶边已经到了屏幕顶上，
/// 这一眼最大化了、鼠标在屏幕最上面。返回窗口原来该放回去的样子（贴着那块屏幕的顶边）。
/// 双击标题栏、Win+↑ 最大化时鼠标不在屏幕最上面，或者窗口原来离顶边还远，不算。
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn snapped_to_top(before: &Seen, now: &Seen) -> Option<Rect> {
    if before.maximized || !now.maximized {
        return None;
    }
    let (cx, cy) = now.cursor;
    let (area, scale) = now
        .screens
        .iter()
        .find(|(a, _)| cx >= a.x && cx < a.x + a.w)
        .copied()?;
    let snap = SNAP * scale;
    let near_top = before.win.y <= area.y + 2.0 * snap;
    let cursor_at_top = cy <= area.y + snap;
    (near_top && cursor_at_top).then_some(Rect {
        y: area.y,
        ..before.win
    })
}

/// 应用启动时开始看着窗口。
pub fn start<R: Runtime>(app: &AppHandle<R>) {
    let app = app.clone();
    std::thread::spawn(move || {
        // 上一眼看到的（判断是不是在拖、Windows 是不是刚贴靠成最大化）；自己挪过窗口以后清空
        let mut last: Option<Seen> = None;
        // 上一眼窗口有没有焦点：一直记着，不跟着 last 清空，免得藏起来以后还有焦点就又弹出来
        let mut was_focused = false;
        loop {
            std::thread::sleep(TICK);
            let edge_hide = app.state::<EdgeHide>();
            if !edge_hide.enabled.load(Ordering::Relaxed) {
                last = None;
                std::thread::sleep(Duration::from_millis(400));
                continue;
            }
            let Some(window) = app.get_window("main") else {
                continue;
            };
            // 先在锁外面把窗口的样子读好
            let Some(seen) = observe(&window) else {
                continue;
            };
            #[cfg(target_os = "windows")]
            if let Some(before) = &last
                && let Some(back) = snapped_to_top(before, &seen)
            {
                tracing::debug!("edge hide: undo snap-to-top maximize");
                let _ = window.unmaximize();
                let _ = window.set_size(tauri::PhysicalSize::new(
                    back.w.round() as u32,
                    back.h.round() as u32,
                ));
                move_to(&window, (back.x, back.y));
                last = None;
                was_focused = seen.focused;
                continue;
            }
            let moving = last.as_ref().is_some_and(|l| l.win != seen.win);
            // 拿着锁只算，不碰窗口
            let target = {
                let mut state = edge_hide.state.lock().unwrap_or_else(|e| e.into_inner());
                step(&mut state, &seen, moving, was_focused, Instant::now()).map(|at| (at, *state))
            };
            was_focused = seen.focused;
            if let Some((at, decided)) = target {
                tracing::debug!(x = at.0, y = at.1, "edge hide: move");
                // 交给主线程挪，挪之前看状态有没有被托盘点击之类改掉（见模块说明）
                let handle = app.clone();
                let (done, moved) = std::sync::mpsc::channel();
                let sent = app.run_on_main_thread(move || {
                    let edge_hide = handle.state::<EdgeHide>();
                    let state = edge_hide.state.lock().unwrap_or_else(|e| e.into_inner());
                    if *state == decided
                        && let Some(window) = handle.get_window("main")
                    {
                        let _ = window.set_position(PhysicalPosition::new(at.0, at.1));
                    }
                    let _ = done.send(());
                });
                // 等主线程挪完再看下一眼（这里没拿着锁，等它不会卡住谁），不然下一眼看到的还是原来的位置，
                // 会当成用户把窗口拖走了
                if sent.is_ok() {
                    let _ = moved.recv_timeout(Duration::from_millis(500));
                }
                // 自己挪的，下一眼别当成用户在拖
                last = None;
            } else {
                last = Some(seen);
            }
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

    fn seen(win: Rect, cursor: (f64, f64)) -> Seen {
        Seen {
            win,
            cursor,
            focused: true,
            usable: true,
            maximized: false,
            screens: vec![(LEFT, 2.0)],
        }
    }

    #[test]
    fn docks_when_the_drag_stops_and_hides_after_the_pointer_leaves() {
        let t0 = Instant::now();
        let mut state = State::Free;
        let docked = win(4.0, 400.0);
        // 还在拖：不管
        assert_eq!(
            step(&mut state, &seen(docked, (300.0, 420.0)), true, true, t0),
            None
        );
        assert_eq!(state, State::Free);
        // 停在左边上：贴上了，鼠标在窗口上
        assert_eq!(
            step(&mut state, &seen(docked, (300.0, 420.0)), false, true, t0),
            None
        );
        assert!(matches!(
            state,
            State::Shown {
                edge: Edge::Left,
                entered: true,
                ..
            }
        ));
        // 鼠标离开：先记下时间，过了一会儿才藏，藏到只露一条边
        let away = seen(docked, (2000.0, 900.0));
        assert_eq!(step(&mut state, &away, false, true, t0), None);
        assert_eq!(
            step(
                &mut state,
                &away,
                false,
                true,
                t0 + Duration::from_millis(100)
            ),
            None
        );
        let at = step(&mut state, &away, false, true, t0 + LEAVE_DELAY);
        assert_eq!(at, Some((-1000 + 8, 400)));
        assert!(matches!(
            state,
            State::Hidden {
                edge: Edge::Left,
                ..
            }
        ));
    }

    #[test]
    fn a_hidden_window_comes_back_when_touched_or_switched_to() {
        let t0 = Instant::now();
        let hidden = win(-992.0, 400.0);
        let hidden_state = State::Hidden {
            edge: Edge::Left,
            area: LEFT,
            scale: 2.0,
            at: (-992, 400),
        };
        // 一直有焦点（刚藏起来时窗口还是当前窗口）：不弹出来
        let mut state = hidden_state;
        let far = seen(hidden, (1500.0, 600.0));
        assert_eq!(step(&mut state, &far, false, true, t0), None);
        assert_eq!(state, hidden_state);
        // Alt+Tab 切回来：弹出来，等鼠标进过窗口再藏
        assert_eq!(step(&mut state, &far, false, false, t0), Some((0, 400)));
        assert!(matches!(state, State::Shown { entered: false, .. }));
        // 鼠标碰到露出来的那条边：弹出来
        let mut state = hidden_state;
        let edge = seen(hidden, (3.0, 600.0));
        assert_eq!(step(&mut state, &edge, false, false, t0), Some((0, 400)));
        // 窗口被挪走了：不再管
        let mut state = hidden_state;
        assert_eq!(
            step(
                &mut state,
                &seen(win(300.0, 300.0), (3.0, 600.0)),
                false,
                false,
                t0
            ),
            None
        );
        assert_eq!(state, State::Free);
        // 最小化、藏到托盘时：藏着的记着，别的状态清掉
        let mut state = hidden_state;
        let mut gone = seen(hidden, (3.0, 600.0));
        gone.usable = false;
        assert_eq!(step(&mut state, &gone, false, false, t0), None);
        assert_eq!(state, hidden_state);
    }

    #[test]
    fn only_a_drag_to_the_top_counts_as_snapping() {
        // 拖着标题栏往上，Windows 把它最大化了
        let before = seen(win(600.0, 30.0), (900.0, 60.0));
        let mut now = seen(win(0.0, 50.0), (900.0, 52.0));
        now.maximized = true;
        let back = snapped_to_top(&before, &now).unwrap();
        assert_eq!(
            (back.x, back.y, back.w, back.h),
            (600.0, 50.0, 1000.0, 700.0)
        );
        // 双击标题栏最大化：窗口原来离顶边还远
        let far = seen(win(600.0, 400.0), (900.0, 420.0));
        assert!(snapped_to_top(&far, &now).is_none());
        // Win+↑：鼠标不在屏幕最上面
        let mut keys = now.clone();
        keys.cursor = (900.0, 700.0);
        assert!(snapped_to_top(&before, &keys).is_none());
        // 本来就是最大化的
        let mut was_max = before.clone();
        was_max.maximized = true;
        assert!(snapped_to_top(&was_max, &now).is_none());
    }

    #[test]
    fn touching_the_strip_reveals() {
        let hidden = win(500.0, -642.0);
        assert!(touches_strip(Edge::Top, hidden, LEFT, 2.0, 800.0, 52.0));
        assert!(!touches_strip(Edge::Top, hidden, LEFT, 2.0, 1600.0, 52.0));
        assert!(!touches_strip(Edge::Top, hidden, LEFT, 2.0, 800.0, 200.0));
    }
}
