//! Przezroczysty proxy PTY: agent rysuje wprost na terminal użytkownika; wrapper
//! może wstrzyknąć tekst do stdin agenta (wiadomości od peerów).
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};

/// Write end of the self-pipe. The SIGWINCH handler writes one byte (async-signal-safe),
/// waking the resize thread blocked on the read end. -1 until the pipe is set up.
static WINCH_PIPE_W: AtomicI32 = AtomicI32::new(-1);

#[cfg(unix)]
extern "C" fn handle_winch(_sig: libc::c_int) {
    let fd = WINCH_PIPE_W.load(Ordering::SeqCst);
    if fd >= 0 {
        let b = 1u8;
        unsafe {
            libc::write(fd, &b as *const u8 as *const libc::c_void, 1);
        }
    }
}

/// Clone-able handle used by the poll thread to inject messages into the agent stdin.
#[derive(Clone)]
pub struct ProxyHandle {
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
}

impl ProxyHandle {
    /// Write `text` into agent stdin, pause 75 ms, then send `\r`.
    pub fn inject(&self, text: &str) {
        {
            let mut w = self.writer.lock().unwrap();
            let _ = w.write_all(text.as_bytes());
            let _ = w.flush();
        }
        std::thread::sleep(Duration::from_millis(75));
        {
            let mut w = self.writer.lock().unwrap();
            let _ = w.write_all(b"\r");
            let _ = w.flush();
        }
    }
}

/// Owned half: holds the child process, master PTY (shared via Arc), and original termios.
/// Call `wait(self)` on the main thread to wait for child exit.
pub struct ProxyChild {
    child: Box<dyn Child + Send + Sync>,
    /// Shared with the SIGWINCH thread so it can resize the PTY on terminal resize.
    master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    orig_termios: Option<libc::termios>,
}

impl ProxyChild {
    /// Wait for the child process to exit, restore terminal, return exit code.
    pub fn wait(mut self) -> i32 {
        let code = self.child.wait().map(|s| s.exit_code() as i32).unwrap_or(1);
        if let Some(orig) = self.orig_termios.take() {
            restore_raw(&orig);
        }
        // master Arc is dropped here; SIGWINCH thread holds its own Arc clone
        // and will fail gracefully on lock once the process exits anyway.
        code
    }

    /// Resize the PTY to match the current terminal size.
    /// resize_to_current remains available for callers; SIGWINCH now drives resizing automatically.
    pub fn resize_to_current(&self) {
        let (rows, cols) = term_size();
        if let Ok(m) = self.master.lock() {
            let _ = m.resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 });
        }
    }
}

/// Entry point: spawns the PTY and returns a (handle, child) pair.
pub struct Proxy;

impl Proxy {
    pub fn spawn(
        command: &[String],
        cwd: &Path,
        env: &[(String, String)],
    ) -> Result<(ProxyHandle, ProxyChild)> {
        let (rows, cols) = term_size();
        let (program, args) = command.split_first().context("empty command")?;
        let pty = native_pty_system();
        let pair = pty
            .openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
            .map_err(|e| anyhow::anyhow!("openpty: {e}"))?;
        let mut cmd = CommandBuilder::new(program);
        cmd.args(args);
        cmd.cwd(cwd);
        // Per-proces zmienne środowiskowe (dziedziczy resztę env rodzica).
        // Używane m.in. przez opencode: config MCP idzie przez OPENCODE_CONFIG_CONTENT, nie flagą.
        for (k, v) in env {
            cmd.env(k, v);
        }
        let child = pair.slave.spawn_command(cmd).map_err(|e| anyhow::anyhow!("spawn: {e}"))?;
        drop(pair.slave);

        // Self-pipe + SIGWINCH handler: the handler writes one byte to the pipe
        // (async-signal-safe), waking the resize thread immediately — no poll latency.
        // (sigwait() does not reliably deliver SIGWINCH here on macOS, hence a handler.)
        #[cfg(unix)]
        let winch_read_fd: libc::c_int = unsafe {
            let mut fds = [0 as libc::c_int; 2];
            if libc::pipe(fds.as_mut_ptr()) == 0 {
                WINCH_PIPE_W.store(fds[1], Ordering::SeqCst);
                let mut sa: libc::sigaction = std::mem::zeroed();
                sa.sa_sigaction = handle_winch as *const () as usize;
                libc::sigemptyset(&mut sa.sa_mask);
                sa.sa_flags = libc::SA_RESTART;
                libc::sigaction(libc::SIGWINCH, &sa, std::ptr::null_mut());
            }
            fds[0]
        };

        // Wrap master in Arc<Mutex<...>> so the SIGWINCH thread can resize it.
        let master_arc: Arc<Mutex<Box<dyn MasterPty + Send>>> =
            Arc::new(Mutex::new(pair.master));

        let mut reader = {
            let m = master_arc.lock().unwrap();
            m.try_clone_reader().map_err(|e| anyhow::anyhow!("reader: {e}"))?
        };
        let writer: Box<dyn Write + Send> = {
            let m = master_arc.lock().unwrap();
            m.take_writer().map_err(|e| anyhow::anyhow!("writer: {e}"))?
        };
        let writer = Arc::new(Mutex::new(writer));

        // master → stdout
        std::thread::spawn(move || {
            let mut out = std::io::stdout();
            let mut buf = [0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if out.write_all(&buf[..n]).is_err() {
                            break;
                        }
                        let _ = out.flush();
                    }
                }
            }
        });

        // Enter raw mode before starting stdin thread (returns None when not a tty).
        let orig_termios = enter_raw();

        // stdin → master
        {
            let writer = Arc::clone(&writer);
            std::thread::spawn(move || {
                let mut inp = std::io::stdin();
                let mut buf = [0u8; 4096];
                loop {
                    match inp.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            let mut w = writer.lock().unwrap();
                            if w.write_all(&buf[..n]).is_err() {
                                break;
                            }
                            let _ = w.flush();
                        }
                    }
                }
            });
        }

        // SIGWINCH thread: blocks on the self-pipe read end; wakes the instant the
        // handler writes, then resizes the PTY to the new terminal size. Runs forever;
        // torn down on process exit.
        #[cfg(unix)]
        {
            let master_arc = Arc::clone(&master_arc);
            std::thread::spawn(move || {
                let mut buf = [0u8; 64];
                loop {
                    // SA_RESTART means read() resumes across signals; a non-positive
                    // return is EOF/error, so we stop.
                    let n = unsafe {
                        libc::read(
                            winch_read_fd,
                            buf.as_mut_ptr() as *mut libc::c_void,
                            buf.len(),
                        )
                    };
                    if n <= 0 {
                        break;
                    }
                    let (rows, cols) = term_size();
                    if let Ok(m) = master_arc.lock() {
                        let _ = m.resize(PtySize {
                            rows,
                            cols,
                            pixel_width: 0,
                            pixel_height: 0,
                        });
                    }
                }
            });
        }

        let handle = ProxyHandle { writer };
        let child_part = ProxyChild { child, master: master_arc, orig_termios };
        Ok((handle, child_part))
    }
}

// ── helpers ──────────────────────────────────────────────────────────────────

fn term_size() -> (u16, u16) {
    #[cfg(unix)]
    unsafe {
        let mut ws: libc::winsize = std::mem::zeroed();
        if libc::ioctl(0, libc::TIOCGWINSZ, &mut ws) == 0 && ws.ws_row > 0 {
            return (ws.ws_row, ws.ws_col);
        }
    }
    (24, 80)
}

fn enter_raw() -> Option<libc::termios> {
    #[cfg(unix)]
    unsafe {
        let mut t: libc::termios = std::mem::zeroed();
        if libc::tcgetattr(0, &mut t) != 0 {
            return None;
        }
        let orig = t;
        libc::cfmakeraw(&mut t);
        libc::tcsetattr(0, libc::TCSANOW, &t);
        return Some(orig);
    }
    #[allow(unreachable_code)]
    None
}

fn restore_raw(orig: &libc::termios) {
    #[cfg(unix)]
    unsafe {
        libc::tcsetattr(0, libc::TCSANOW, orig);
    }
}
