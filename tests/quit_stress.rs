//! Regression test for "leave it open for a while and it won't quit" /
//! "quit still feels stuck": drives the real compiled TUI binary inside an
//! actual pty (so crossterm's raw-mode/event parsing runs for real, not
//! just the Rust-level logic), floods it with a backlog of legitimate
//! drag-mouse events the way a terminal replaying buffered input would,
//! then sends a quit key and asserts the process actually exits within a
//! bounded time.
//!
//! This test is what found a real bug: crossterm's default Unix event
//! backend (`mio`/epoll) registers the tty fd edge-triggered, but its read
//! loop returns as soon as one event becomes parseable instead of draining
//! the fd to `EAGAIN` first. Once more than ~1024 bytes of input arrive in
//! a single burst (this flood, but also a real terminal replaying enough
//! buffered events), everything after the first ~1024-byte read is left
//! unread in the kernel buffer, and — because the edge was already
//! consumed — epoll never wakes up for it again. The app then blocks in
//! `epoll_wait` forever, having silently dropped every event past that
//! point, including whatever quit key came after. Fixed by switching to
//! crossterm's `use-dev-tty` feature, which uses level-triggered `poll(2)`
//! instead (see the comment on the crossterm dependency in Cargo.toml).
//!
//! Unit-level reasoning about our own event loop could never have caught
//! this — it's a real interaction between our code, crossterm, and the
//! kernel's epoll edge-triggering, only visible when actually driving the
//! compiled binary through a real pty under load.
//!
//! Unix-only: the pty setup below is POSIX-specific (`openpty`, `setsid`,
//! `TIOCSCTTY`). There's no equivalent low-level pty API to drive a real
//! Windows console session from a test, so this doesn't attempt one.

#![cfg(unix)]

use std::io::{Read, Write};
use std::os::unix::io::{FromRawFd, RawFd};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// How many synthetic drag-mouse events to inject before the quit key.
/// The bug this guards against needs only ~1024 bytes (roughly 80 of
/// these ~13-byte events) to reproduce, so this is comfortably more than
/// enough to cross that boundary several times over — a debug-build
/// child process's own consumption speed ends up pacing how fast this can
/// actually be written (the kernel pty buffer isn't infinite), so keeping
/// this modest matters for the test's wall-clock time, not just for
/// reproduction reliability.
const FLOOD_EVENTS: usize = 500;

/// How long the process gets, after the quit key is sent, before the test
/// fails it as stuck. Generous enough not to be flaky in a loaded CI
/// sandbox, tight enough that a reintroduced "one full redraw per queued
/// event" regression against a nontrivial tree would plausibly blow past
/// it well before this many events finish draining.
const QUIT_TIMEOUT: Duration = Duration::from_secs(10);

/// Builds a small-but-nontrivial directory tree (a few thousand files
/// across several subdirectories) so each redraw the app performs during
/// the test has real, if modest, list-sort and treemap-layout work to do
/// — a completely empty directory would make a "redraw per queued event"
/// regression too cheap to ever time out on, defeating the point of the
/// flood.
fn make_test_tree() -> std::path::PathBuf {
    let mut dir = std::env::temp_dir();
    dir.push(format!(
        "rustdirstat-quit-stress-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    for sub in 0..20 {
        let subdir = dir.join(format!("sub_{sub:03}"));
        std::fs::create_dir_all(&subdir).expect("create subdir");
        for f in 0..150 {
            std::fs::write(subdir.join(format!("file_{f:03}.txt")), b"x".repeat(64))
                .expect("write file");
        }
    }
    dir
}

/// Opens a pty, spawns the built `rustdirstat` binary attached to it as
/// its controlling terminal (matching how a real interactive terminal
/// session attaches to a program), and returns the child plus the pty
/// master fd for writing synthetic input / draining output.
fn spawn_in_pty(target: &std::path::Path) -> (Child, RawFd) {
    let mut master: RawFd = -1;
    let mut slave: RawFd = -1;
    // A real, non-degenerate size — without this the pty defaults to 0x0,
    // which starves the app's list/treemap rendering of anything
    // meaningful to lay out and makes the flood a much weaker test of
    // real redraw behavior.
    let winsize = libc::winsize {
        ws_row: 50,
        ws_col: 200,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let rc = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null(),
            &winsize,
        )
    };
    assert_eq!(rc, 0, "openpty failed: {}", std::io::Error::last_os_error());

    let dup_slave = || unsafe {
        let fd = libc::dup(slave);
        assert!(
            fd >= 0,
            "dup(slave) failed: {}",
            std::io::Error::last_os_error()
        );
        Stdio::from(std::fs::File::from_raw_fd(fd))
    };

    let bin = env!("CARGO_BIN_EXE_rustdirstat");
    let mut cmd = Command::new(bin);
    cmd.arg(target)
        .env("TERM", "xterm-256color")
        .stdin(dup_slave())
        .stdout(dup_slave())
        .stderr(dup_slave());

    // Make the pty slave the child's controlling terminal — without this,
    // crossterm's isatty/terminal-mode setup on the slave fd doesn't
    // behave the way it would for a real interactively-attached terminal.
    unsafe {
        cmd.pre_exec(|| {
            if libc::setsid() < 0 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::ioctl(0, libc::TIOCSCTTY as _, 0) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }

    let child = cmd.spawn().expect("spawn rustdirstat");
    unsafe {
        libc::close(slave);
    }
    (child, master)
}

/// Continuously drains the pty master in the background so the child
/// never blocks on a full output buffer while we're busy writing a large
/// input flood — a real terminal would be reading the whole time too.
fn spawn_output_drain(master: RawFd) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut file = unsafe { std::fs::File::from_raw_fd(libc::dup(master)) };
        let mut buf = [0u8; 8192];
        loop {
            match file.read(&mut buf) {
                Ok(0) => break,
                Ok(_) => {}
                Err(_) => break,
            }
        }
    })
}

fn wait_for_exit(child: &mut Child, timeout: Duration) -> Option<std::process::ExitStatus> {
    let started = Instant::now();
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            return Some(status);
        }
        if started.elapsed() >= timeout {
            return None;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Writes a large burst of SGR-encoded mouse "drag" events (button 1
/// held, motion reported) — mode 1002, which the app deliberately keeps
/// enabled for the treemap-resize handle — followed by the given trailing
/// bytes (the actual quit input).
fn flood_then_send(master: RawFd, trailing: &[u8]) {
    let mut file = unsafe { std::fs::File::from_raw_fd(libc::dup(master)) };
    let mut payload = Vec::with_capacity(FLOOD_EVENTS * 16 + trailing.len());
    for i in 0..FLOOD_EVENTS {
        let col = 10 + (i % 60);
        let row = 5 + (i % 20);
        payload.extend_from_slice(format!("\x1b[<32;{col};{row}M").as_bytes());
    }
    payload.extend_from_slice(trailing);
    // Best-effort: a pty input buffer can be smaller than this whole
    // payload, so short writes are expected and fine — the drain thread
    // on the other side keeps consuming, so this just paces itself.
    let mut written = 0;
    while written < payload.len() {
        match file.write(&payload[written..]) {
            Ok(0) => break,
            Ok(n) => written += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => panic!("write to pty failed: {e}"),
        }
    }
}

fn cleanup(dir: &std::path::Path) {
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn quit_key_works_after_large_event_backlog() {
    let dir = make_test_tree();
    let (mut child, master) = spawn_in_pty(&dir);
    let _drain = spawn_output_drain(master);

    // Give the app a moment to finish its initial scan and reach the
    // interactive browse loop before flooding it.
    std::thread::sleep(Duration::from_millis(500));

    flood_then_send(master, b"q");

    let status = wait_for_exit(&mut child, QUIT_TIMEOUT);
    if status.is_none() {
        let _ = child.kill();
        let _ = child.wait();
    }
    cleanup(&dir);
    assert!(
        status.is_some(),
        "rustdirstat did not exit within {:?} after 'q' following a {}-event backlog",
        QUIT_TIMEOUT,
        FLOOD_EVENTS
    );
    // `is_some()` alone only proves the process died within the timeout —
    // a regression that made the flood *crash* the app (e.g. a panic
    // reading stale state) would exit just as fast and pass that check,
    // even though this test is specifically guarding "the quit key
    // works," not "the app doesn't crash." A clean quit returns Ok(())
    // from main, so it should exit 0; a Rust panic exits 101 instead.
    assert!(
        status.unwrap().success(),
        "rustdirstat exited with a failure status after 'q' — it died \
         (e.g. panicked) rather than quitting cleanly: {:?}",
        status
    );
}

#[test]
fn ctrl_c_works_after_large_event_backlog_and_overrides_help_popup() {
    let dir = make_test_tree();
    let (mut child, master) = spawn_in_pty(&dir);
    let _drain = spawn_output_drain(master);

    std::thread::sleep(Duration::from_millis(500));

    // Open the help popup first — Ctrl+C must still quit immediately
    // regardless of modal state, which is exactly the earlier bug (Ctrl+C
    // was never bound to any action at all).
    {
        let mut file = unsafe { std::fs::File::from_raw_fd(libc::dup(master)) };
        file.write_all(b"?").expect("send '?' to open help");
    }
    std::thread::sleep(Duration::from_millis(100));

    // Ctrl+C is byte 0x03 in raw mode.
    flood_then_send(master, &[0x03]);

    let status = wait_for_exit(&mut child, QUIT_TIMEOUT);
    if status.is_none() {
        let _ = child.kill();
        let _ = child.wait();
    }
    cleanup(&dir);
    assert!(
        status.is_some(),
        "rustdirstat did not exit within {:?} after Ctrl+C following a {}-event backlog \
         (with the help popup open)",
        QUIT_TIMEOUT,
        FLOOD_EVENTS
    );
    // See the matching assertion in quit_key_works_after_large_event_backlog
    // for why process death alone isn't sufficient.
    assert!(
        status.unwrap().success(),
        "rustdirstat exited with a failure status after Ctrl+C — it died \
         (e.g. panicked) rather than quitting cleanly: {:?}",
        status
    );
}
