use std::ffi::{c_char, c_void, CString};
use std::process::Command;
use std::ptr;

type Id = *mut c_void;
type Sel = *mut c_void;
type Class = *mut c_void;

#[link(name = "objc", kind = "dylib")]
extern "C" {
    fn objc_getClass(name: *const c_char) -> Class;
    fn sel_registerName(name: *const c_char) -> Sel;
    fn objc_msgSend();
    fn objc_allocateClassPair(superclass: Class, name: *const c_char, extra: usize) -> Class;
    fn class_addMethod(cls: Class, sel: Sel, imp: *const c_void, types: *const c_char) -> bool;
    fn objc_registerClassPair(cls: Class);
}

type IOPMAssertionID = u32;

#[link(name = "IOKit", kind = "framework")]
extern "C" {
    fn IOPMAssertionCreateWithName(
        ty: *const c_void,
        level: u32,
        name: *const c_void,
        id: *mut IOPMAssertionID,
    ) -> i32;
    fn IOPMAssertionRelease(id: IOPMAssertionID) -> i32;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFStringCreateWithCString(
        alloc: *const c_void,
        s: *const c_char,
        enc: u32,
    ) -> *const c_void;
    fn CFRelease(cf: *const c_void);
}

fn cls(name: &str) -> Class {
    let c = CString::new(name).unwrap();
    unsafe { objc_getClass(c.as_ptr()) }
}

fn sel(name: &str) -> Sel {
    let c = CString::new(name).unwrap();
    unsafe { sel_registerName(c.as_ptr()) }
}

fn cfstr(s: &str) -> *const c_void {
    let c = CString::new(s).unwrap();
    unsafe { CFStringCreateWithCString(ptr::null(), c.as_ptr(), 0x08000100) }
}

fn nsstr(s: &str) -> Id {
    let c = CString::new(s).unwrap();
    unsafe { send1(cls("NSString") as Id, sel("stringWithUTF8String:"), c.as_ptr() as Id) }
}

unsafe fn send0(obj: Id, s: Sel) -> Id {
    let f: extern "C" fn(Id, Sel) -> Id = std::mem::transmute(objc_msgSend as *const ());
    f(obj, s)
}

unsafe fn send1(obj: Id, s: Sel, a: Id) -> Id {
    let f: extern "C" fn(Id, Sel, Id) -> Id = std::mem::transmute(objc_msgSend as *const ());
    f(obj, s, a)
}

unsafe fn send1_i64(obj: Id, s: Sel, a: i64) -> Id {
    let f: extern "C" fn(Id, Sel, i64) -> Id = std::mem::transmute(objc_msgSend as *const ());
    f(obj, s, a)
}

unsafe fn send1_f64(obj: Id, s: Sel, a: f64) -> Id {
    let f: extern "C" fn(Id, Sel, f64) -> Id = std::mem::transmute(objc_msgSend as *const ());
    f(obj, s, a)
}

unsafe fn send3(obj: Id, s: Sel, a: Id, b: Id, c: Id) -> Id {
    let f: extern "C" fn(Id, Sel, Id, Id, Id) -> Id = std::mem::transmute(objc_msgSend as *const ());
    f(obj, s, a, b, c)
}

fn menu_item(title: &str, action: Sel, key: &str) -> Id {
    unsafe {
        send3(
            send0(cls("NSMenuItem") as Id, sel("alloc")),
            sel("initWithTitle:action:keyEquivalent:"),
            nsstr(title),
            action as Id,
            nsstr(key),
        )
    }
}

static mut SYS_ID: IOPMAssertionID = 0;
static mut DSP_ID: IOPMAssertionID = 0;
static mut AWAKE: bool = false;
static mut LID_OFF: bool = false;
static mut AWAKE_ITEM: Id = ptr::null_mut();
static mut LID_ITEM: Id = ptr::null_mut();
static mut BTN: Id = ptr::null_mut();

fn create_assertions() -> bool {
    unsafe {
        let name = cfstr("awake");
        let sys_ty = cfstr("PreventUserIdleSystemSleep");
        let ok = IOPMAssertionCreateWithName(sys_ty, 255, name, &raw mut SYS_ID) == 0;
        CFRelease(sys_ty);
        if ok {
            let dsp_ty = cfstr("PreventUserIdleDisplaySleep");
            IOPMAssertionCreateWithName(dsp_ty, 255, name, &raw mut DSP_ID);
            CFRelease(dsp_ty);
        }
        CFRelease(name);
        ok
    }
}

fn release_assertions() {
    unsafe {
        IOPMAssertionRelease(SYS_ID);
        IOPMAssertionRelease(DSP_ID);
        SYS_ID = 0;
        DSP_ID = 0;
    }
}

fn update_icon() {
    unsafe {
        let icon = if AWAKE { "☕" } else { "💤" };
        send1(BTN, sel("setTitle:"), nsstr(icon));
    }
}

extern "C" fn toggle_awake(_this: Id, _cmd: Sel, _sender: Id) {
    unsafe {
        if AWAKE {
            release_assertions();
            AWAKE = false;
            send1_i64(AWAKE_ITEM, sel("setState:"), 0);
        } else if create_assertions() {
            AWAKE = true;
            send1_i64(AWAKE_ITEM, sel("setState:"), 1);
        }
        update_icon();
    }
}

extern "C" fn toggle_lid(_this: Id, _cmd: Sel, _sender: Id) {
    unsafe {
        if LID_OFF {
            if run_priv("pmset -a disablesleep 0") {
                LID_OFF = false;
                send1_i64(LID_ITEM, sel("setState:"), 0);
            }
        } else if run_priv("pmset -a disablesleep 1") {
            LID_OFF = true;
            send1_i64(LID_ITEM, sel("setState:"), 1);
        }
    }
}

extern "C" fn do_quit(_this: Id, _cmd: Sel, _sender: Id) {
    unsafe {
        if AWAKE {
            release_assertions();
        }
        if LID_OFF {
            let _ = Command::new("osascript")
                .args([
                    "-e",
                    "do shell script \"pmset -a disablesleep 0\" with administrator privileges",
                ])
                .status();
        }
    }
    std::process::exit(0);
}

fn run_priv(cmd: &str) -> bool {
    Command::new("osascript")
        .arg("-e")
        .arg(format!(
            "do shell script \"{}\" with administrator privileges",
            cmd
        ))
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn check_disablesleep() -> bool {
    Command::new("pmset")
        .arg("-g")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| {
            s.lines()
                .any(|l| l.contains("disablesleep") && l.trim().ends_with('1'))
        })
        .unwrap_or(false)
}

pub fn run() {
    unsafe {
        AWAKE = create_assertions();
        LID_OFF = check_disablesleep();

        let app = send0(cls("NSApplication") as Id, sel("sharedApplication"));
        send1_i64(app, sel("setActivationPolicy:"), 1);

        let bar = send0(cls("NSStatusBar") as Id, sel("systemStatusBar"));
        let item = send1_f64(bar, sel("statusItemWithLength:"), -1.0);
        BTN = send0(item, sel("button"));
        update_icon();

        let del_cls = objc_allocateClassPair(cls("NSObject"), c"AwakeDelegate".as_ptr(), 0);
        class_addMethod(del_cls, sel("toggleAwake:"), toggle_awake as *const c_void, c"v@:@".as_ptr());
        class_addMethod(del_cls, sel("toggleLid:"), toggle_lid as *const c_void, c"v@:@".as_ptr());
        class_addMethod(del_cls, sel("quit:"), do_quit as *const c_void, c"v@:@".as_ptr());
        objc_registerClassPair(del_cls);
        let delegate = send0(send0(del_cls as Id, sel("alloc")), sel("init"));

        let menu = send0(send0(cls("NSMenu") as Id, sel("alloc")), sel("init"));

        AWAKE_ITEM = menu_item("Stay Awake", sel("toggleAwake:"), "");
        send1(AWAKE_ITEM, sel("setTarget:"), delegate);
        if AWAKE {
            send1_i64(AWAKE_ITEM, sel("setState:"), 1);
        }
        send1(menu, sel("addItem:"), AWAKE_ITEM);

        LID_ITEM = menu_item("Prevent Lid-Close Sleep", sel("toggleLid:"), "");
        send1(LID_ITEM, sel("setTarget:"), delegate);
        if LID_OFF {
            send1_i64(LID_ITEM, sel("setState:"), 1);
        }
        send1(menu, sel("addItem:"), LID_ITEM);

        send1(menu, sel("addItem:"), send0(cls("NSMenuItem") as Id, sel("separatorItem")));

        let q = menu_item("Quit", sel("quit:"), "q");
        send1(q, sel("setTarget:"), delegate);
        send1(menu, sel("addItem:"), q);

        send1(item, sel("setMenu:"), menu);
        send0(app, sel("run"));
    }
}
