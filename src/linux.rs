use std::ffi::c_void;
use std::io::{self, Write};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};

static RUNNING: AtomicBool = AtomicBool::new(true);

extern "C" fn on_signal(_: std::ffi::c_int) {
    RUNNING.store(false, Ordering::Relaxed);
}

extern "C" {
    fn signal(sig: std::ffi::c_int, handler: extern "C" fn(std::ffi::c_int)) -> *const c_void;
}

fn inhibit(what: &str, why: &str) -> Option<Child> {
    Command::new("systemd-inhibit")
        .args([
            &format!("--what={}", what),
            "--who=awake",
            &format!("--why={}", why),
            "--mode=block",
            "cat",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()
}

fn ask(prompt: &str) -> bool {
    print!("{prompt} [y/N]: ");
    io::stdout().flush().unwrap();
    let mut buf = String::new();
    io::stdin().read_line(&mut buf).unwrap();
    matches!(buf.trim(), "y" | "Y" | "yes")
}

pub fn run() {
    unsafe {
        signal(2, on_signal);
        signal(15, on_signal);
    }

    let sleep_child = inhibit("idle:sleep", "Preventing sleep");
    let has_systemd = sleep_child.is_some();

    if has_systemd {
        println!("awake - preventing idle sleep");
    } else {
        println!("awake - systemd-inhibit not available");
        println!("       (sleep prevention requires systemd)");
    }

    let has_x11 = std::env::var("DISPLAY").is_ok();
    if has_x11 {
        let _ = Command::new("xset").args(["s", "off"]).status();
        let _ = Command::new("xset").args(["-dpms"]).status();
        println!("       display will stay on (X11)");
    }

    println!();

    let mut lid_child: Option<Child> = None;

    if has_systemd {
        println!("lid-close: system may sleep when you close the lid");
        if ask("inhibit lid-close sleep?") {
            match inhibit("handle-lid-switch", "Preventing lid-close sleep") {
                Some(c) => {
                    lid_child = Some(c);
                    println!("done - lid close inhibited");
                }
                None => {
                    eprintln!("failed (may need root)");
                    eprintln!("  try: sudo systemd-inhibit --what=handle-lid-switch sleep infinity &");
                    eprintln!("  or:  edit /etc/systemd/logind.conf → HandleLidSwitch=ignore");
                }
            }
        } else {
            println!("to do it manually:");
            println!("  systemd-inhibit --what=handle-lid-switch sleep infinity &");
            println!("  or edit /etc/systemd/logind.conf → HandleLidSwitch=ignore");
        }
    }

    println!("\nctrl+c to stop");

    while RUNNING.load(Ordering::Relaxed) {
        std::thread::sleep(std::time::Duration::from_millis(500));
    }

    if let Some(mut c) = sleep_child {
        drop(c.stdin.take());
        let _ = c.wait();
    }
    if let Some(mut c) = lid_child {
        drop(c.stdin.take());
        let _ = c.wait();
    }

    if has_x11 {
        let _ = Command::new("xset").args(["s", "on"]).status();
        let _ = Command::new("xset").args(["+dpms"]).status();
    }

    println!("\ndone");
}
