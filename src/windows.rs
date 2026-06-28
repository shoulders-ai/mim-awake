use std::ffi::c_void;
use std::process::Command;
use std::ptr;

type HWND = *mut c_void;
type HICON = *mut c_void;
type HMENU = *mut c_void;
type HINSTANCE = *mut c_void;

const WM_APP: u32 = 0x8000;
const WM_COMMAND: u32 = 0x0111;
const WM_DESTROY: u32 = 0x0002;
const WM_RBUTTONUP: u32 = 0x0205;
const WM_LBUTTONUP: u32 = 0x0202;

const NIM_ADD: u32 = 0;
const NIM_DELETE: u32 = 2;
const NIF_ICON: u32 = 2;
const NIF_MESSAGE: u32 = 1;
const NIF_TIP: u32 = 4;

const MF_STRING: u32 = 0;
const MF_SEPARATOR: u32 = 0x800;
const MF_CHECKED: u32 = 0x8;
const MF_GRAYED: u32 = 0x1;
const TPM_RIGHTBUTTON: u32 = 2;

const ES_CONTINUOUS: u32 = 0x80000000;
const ES_SYSTEM_REQUIRED: u32 = 0x00000001;
const ES_DISPLAY_REQUIRED: u32 = 0x00000002;

const ID_TOGGLE_LID: usize = 1;
const ID_QUIT: usize = 2;

#[repr(C)]
struct WndClassW {
    style: u32,
    wnd_proc: unsafe extern "system" fn(HWND, u32, usize, isize) -> isize,
    cls_extra: i32,
    wnd_extra: i32,
    instance: HINSTANCE,
    icon: HICON,
    cursor: *mut c_void,
    background: *mut c_void,
    menu_name: *const u16,
    class_name: *const u16,
}

#[repr(C)]
struct NotifyIconData {
    cb_size: u32,
    hwnd: HWND,
    uid: u32,
    flags: u32,
    callback_message: u32,
    icon: HICON,
    tip: [u16; 128],
    state: u32,
    state_mask: u32,
    info: [u16; 256],
    timeout_version: u32,
    info_title: [u16; 64],
    info_flags: u32,
    guid: [u8; 16],
    balloon_icon: HICON,
}

#[repr(C)]
struct Point {
    x: i32,
    y: i32,
}

#[repr(C)]
struct Msg {
    hwnd: HWND,
    message: u32,
    wparam: usize,
    lparam: isize,
    time: u32,
    pt: Point,
}

#[link(name = "kernel32")]
#[link(name = "user32")]
#[link(name = "shell32")]
extern "system" {
    fn SetThreadExecutionState(flags: u32) -> u32;
    fn GetModuleHandleW(name: *const u16) -> HINSTANCE;
    fn RegisterClassW(wc: *const WndClassW) -> u16;
    fn CreateWindowExW(
        ex: u32, class: *const u16, title: *const u16, style: u32,
        x: i32, y: i32, w: i32, h: i32,
        parent: HWND, menu: HMENU, inst: HINSTANCE, param: *mut c_void,
    ) -> HWND;
    fn DefWindowProcW(hwnd: HWND, msg: u32, wp: usize, lp: isize) -> isize;
    fn GetMessageW(msg: *mut Msg, hwnd: HWND, min: u32, max: u32) -> i32;
    fn TranslateMessage(msg: *const Msg) -> i32;
    fn DispatchMessageW(msg: *const Msg) -> isize;
    fn PostQuitMessage(code: i32);
    fn LoadIconW(inst: HINSTANCE, name: *const u16) -> HICON;
    fn CreatePopupMenu() -> HMENU;
    fn AppendMenuW(menu: HMENU, flags: u32, id: usize, text: *const u16) -> i32;
    fn TrackPopupMenu(
        menu: HMENU, flags: u32, x: i32, y: i32, reserved: i32,
        hwnd: HWND, rect: *const c_void,
    ) -> i32;
    fn DestroyMenu(menu: HMENU) -> i32;
    fn GetCursorPos(pt: *mut Point) -> i32;
    fn SetForegroundWindow(hwnd: HWND) -> i32;
    fn Shell_NotifyIconW(msg: u32, data: *mut NotifyIconData) -> i32;
    fn ShellExecuteW(
        hwnd: HWND, op: *const u16, file: *const u16, params: *const u16,
        dir: *const u16, show: i32,
    ) -> HINSTANCE;
}

fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn set_tip(buf: &mut [u16; 128], s: &str) {
    for (i, c) in s.encode_utf16().take(127).enumerate() {
        buf[i] = c;
    }
}

static mut LID_OFF: bool = false;
static mut MAIN_HWND: HWND = ptr::null_mut();
static mut ORIG_AC: u32 = 1;
static mut ORIG_DC: u32 = 1;

fn get_lid_action() -> (u32, u32) {
    let out = Command::new("powercfg")
        .args(["/QUERY", "SCHEME_CURRENT", "SUB_BUTTONS", "LIDACTION"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();

    let mut ac = 1u32;
    let mut dc = 1u32;
    for line in out.lines() {
        if let Some(hex) = line.strip_suffix(|_: char| true).and(None).or_else(|| {
            if line.contains("Current AC Power Setting Index") {
                line.rsplit("0x").next()
            } else {
                None
            }
        }) {
            ac = u32::from_str_radix(hex.trim(), 16).unwrap_or(1);
        }
        if let Some(hex) = line.rsplit("0x").next() {
            if line.contains("Current DC Power Setting Index") {
                dc = u32::from_str_radix(hex.trim(), 16).unwrap_or(1);
            }
            if line.contains("Current AC Power Setting Index") {
                ac = u32::from_str_radix(hex.trim(), 16).unwrap_or(1);
            }
        }
    }
    (ac, dc)
}

fn set_lid_action(ac: u32, dc: u32) {
    let args = format!(
        "/c powercfg /SETACVALUEINDEX SCHEME_CURRENT SUB_BUTTONS LIDACTION {} & \
         powercfg /SETDCVALUEINDEX SCHEME_CURRENT SUB_BUTTONS LIDACTION {} & \
         powercfg /SETACTIVE SCHEME_CURRENT",
        ac, dc
    );
    unsafe {
        ShellExecuteW(
            ptr::null_mut(),
            w("runas").as_ptr(),
            w("cmd").as_ptr(),
            w(&args).as_ptr(),
            ptr::null(),
            0,
        );
    }
}

fn show_menu(hwnd: HWND) {
    unsafe {
        let menu = CreatePopupMenu();

        AppendMenuW(menu, MF_STRING | MF_GRAYED, 0, w("Preventing Sleep").as_ptr());
        AppendMenuW(menu, MF_SEPARATOR, 0, ptr::null());

        let lid_flags = MF_STRING | if LID_OFF { MF_CHECKED } else { 0 };
        AppendMenuW(menu, lid_flags, ID_TOGGLE_LID, w("Prevent Lid-Close Sleep").as_ptr());
        AppendMenuW(menu, MF_SEPARATOR, 0, ptr::null());

        AppendMenuW(menu, MF_STRING, ID_QUIT, w("Quit").as_ptr());

        let mut pt = Point { x: 0, y: 0 };
        GetCursorPos(&mut pt);
        SetForegroundWindow(hwnd);
        TrackPopupMenu(menu, TPM_RIGHTBUTTON, pt.x, pt.y, 0, hwnd, ptr::null());
        DestroyMenu(menu);
    }
}

fn cleanup() {
    unsafe {
        SetThreadExecutionState(ES_CONTINUOUS);
        if LID_OFF {
            set_lid_action(ORIG_AC, ORIG_DC);
        }
        let mut nid: NotifyIconData = std::mem::zeroed();
        nid.cb_size = std::mem::size_of::<NotifyIconData>() as u32;
        nid.hwnd = MAIN_HWND;
        nid.uid = 1;
        Shell_NotifyIconW(NIM_DELETE, &mut nid);
    }
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wp: usize, lp: isize) -> isize {
    match msg {
        WM_APP => {
            let event = lp as u32;
            if event == WM_RBUTTONUP || event == WM_LBUTTONUP {
                show_menu(hwnd);
            }
            0
        }
        WM_COMMAND => {
            match wp & 0xFFFF {
                1 => {
                    // toggle lid
                    if LID_OFF {
                        set_lid_action(ORIG_AC, ORIG_DC);
                        LID_OFF = false;
                    } else {
                        let (ac, dc) = get_lid_action();
                        ORIG_AC = ac;
                        ORIG_DC = dc;
                        set_lid_action(0, 0);
                        LID_OFF = true;
                    }
                }
                2 => {
                    cleanup();
                    PostQuitMessage(0);
                }
                _ => {}
            }
            0
        }
        WM_DESTROY => {
            cleanup();
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

pub fn run() {
    unsafe {
        SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED);

        let (ac, dc) = get_lid_action();
        ORIG_AC = ac;
        ORIG_DC = dc;
        LID_OFF = ac == 0 && dc == 0;

        let inst = GetModuleHandleW(ptr::null());
        let class_name = w("awake");

        let wc = WndClassW {
            style: 0,
            wnd_proc,
            cls_extra: 0,
            wnd_extra: 0,
            instance: inst,
            icon: ptr::null_mut(),
            cursor: ptr::null_mut(),
            background: ptr::null_mut(),
            menu_name: ptr::null(),
            class_name: class_name.as_ptr(),
        };
        RegisterClassW(&wc);

        MAIN_HWND = CreateWindowExW(
            0,
            class_name.as_ptr(),
            w("Awake").as_ptr(),
            0,
            0, 0, 0, 0,
            ptr::null_mut(),
            ptr::null_mut(),
            inst,
            ptr::null_mut(),
        );

        let icon = LoadIconW(ptr::null_mut(), 32512 as *const u16); // IDI_APPLICATION

        let mut nid: NotifyIconData = std::mem::zeroed();
        nid.cb_size = std::mem::size_of::<NotifyIconData>() as u32;
        nid.hwnd = MAIN_HWND;
        nid.uid = 1;
        nid.flags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
        nid.callback_message = WM_APP;
        nid.icon = icon;
        set_tip(&mut nid.tip, "Awake - preventing sleep");

        Shell_NotifyIconW(NIM_ADD, &mut nid);

        let mut msg: Msg = std::mem::zeroed();
        while GetMessageW(&mut msg, ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
