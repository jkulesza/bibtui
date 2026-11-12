pub mod components;
pub mod event;
pub mod keybindings;
pub mod screens;
pub mod theme;

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use anyhow::Result;
use crossterm::cursor::Show;
use crossterm::event::{
    DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

pub type Term = Terminal<CrosstermBackend<io::Stdout>>;

/// A terminal mode bibtui turns on for the TUI, in setup order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalStep {
    RawMode,
    AlternateScreen,
    MouseCapture,
    BracketedPaste,
}

impl TerminalStep {
    pub const ALL: [TerminalStep; 4] = [
        TerminalStep::RawMode,
        TerminalStep::AlternateScreen,
        TerminalStep::MouseCapture,
        TerminalStep::BracketedPaste,
    ];
}

/// Turns terminal modes on and off (the real terminal, or a fake in tests).
pub trait TerminalControl {
    /// Whether this controls the process's own terminal, whose state the
    /// panic hook needs to know.
    const PROCESS_TERMINAL: bool = false;
    fn enable(&mut self, step: TerminalStep) -> io::Result<()>;
    fn disable(&mut self, step: TerminalStep) -> io::Result<()>;
    fn show_cursor(&mut self) -> io::Result<()>;
}

/// The process's terminal, through crossterm.
pub struct Crossterm;

impl TerminalControl for Crossterm {
    const PROCESS_TERMINAL: bool = true;

    fn enable(&mut self, step: TerminalStep) -> io::Result<()> {
        let mut out = io::stdout();
        match step {
            TerminalStep::RawMode => enable_raw_mode(),
            TerminalStep::AlternateScreen => execute!(out, EnterAlternateScreen),
            TerminalStep::MouseCapture => execute!(out, EnableMouseCapture),
            TerminalStep::BracketedPaste => execute!(out, EnableBracketedPaste),
        }
    }

    fn disable(&mut self, step: TerminalStep) -> io::Result<()> {
        let mut out = io::stdout();
        match step {
            TerminalStep::RawMode => disable_raw_mode(),
            TerminalStep::AlternateScreen => execute!(out, LeaveAlternateScreen),
            TerminalStep::MouseCapture => execute!(out, DisableMouseCapture),
            TerminalStep::BracketedPaste => execute!(out, DisableBracketedPaste),
        }
    }

    fn show_cursor(&mut self) -> io::Result<()> {
        execute!(io::stdout(), Show)
    }
}

/// Whether the TUI currently owns the terminal (for the panic hook).
static TERMINAL_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Undoes exactly the terminal modes it turned on, in reverse order, when
/// [`restore`](Self::restore)d or dropped, so an error or panic partway
/// through setup or the event loop never leaves the terminal in raw mode or
/// on the alternate screen.
pub struct TerminalGuard<C: TerminalControl = Crossterm> {
    control: C,
    enabled: Vec<TerminalStep>,
    restored: bool,
}

impl<C: TerminalControl> TerminalGuard<C> {
    /// Turn on every mode the TUI needs. If one fails, the ones already on
    /// are turned off again (when the guard is dropped by `?`).
    pub fn enable(control: C) -> io::Result<Self> {
        let mut guard = TerminalGuard {
            control,
            enabled: Vec::new(),
            restored: false,
        };
        if C::PROCESS_TERMINAL {
            TERMINAL_ACTIVE.store(true, Ordering::SeqCst);
        }
        for step in TerminalStep::ALL {
            guard.control.enable(step)?;
            guard.enabled.push(step);
        }
        Ok(guard)
    }

    /// Turn the modes off again, attempting every step even if one fails,
    /// and show the cursor. Returns the first error. Restoring twice does
    /// nothing the second time.
    pub fn restore(&mut self) -> io::Result<()> {
        if self.restored {
            return Ok(());
        }
        self.restored = true;
        let mut first_error = None;
        while let Some(step) = self.enabled.pop() {
            if let Err(e) = self.control.disable(step) {
                first_error.get_or_insert(e);
            }
        }
        if let Err(e) = self.control.show_cursor() {
            first_error.get_or_insert(e);
        }
        if C::PROCESS_TERMINAL {
            TERMINAL_ACTIVE.store(false, Ordering::SeqCst);
        }
        first_error.map_or(Ok(()), Err)
    }

    #[cfg(test)]
    fn control(&self) -> &C {
        &self.control
    }
}

impl<C: TerminalControl> Drop for TerminalGuard<C> {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

/// Set up the terminal for the TUI: the guard that restores it, and the
/// ratatui terminal to draw on.
pub fn setup_terminal() -> Result<(TerminalGuard, Term)> {
    let guard = TerminalGuard::enable(Crossterm)?;
    let terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    Ok((guard, terminal))
}

/// The event loop's `result`, after `cleanup` (restoring the terminal). The
/// loop's error is kept when both fail; a cleanup failure after a clean exit
/// is the error. Returns the cleanup error to report separately, if any.
pub fn finish(result: Result<()>, cleanup: io::Result<()>) -> (Result<()>, Option<io::Error>) {
    match (result, cleanup) {
        (Err(e), Err(c)) => (Err(e), Some(c)),
        (Ok(()), Err(c)) => (
            Err(anyhow::Error::new(c).context("restoring the terminal")),
            None,
        ),
        (result, Ok(())) => (result, None),
    }
}

/// Messages of panics in background threads, shown after the TUI exits.
static WORKER_PANICS: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// What the panic hook does for a panic.
#[derive(Debug, PartialEq, Eq)]
enum PanicAction {
    /// Restore the terminal, then report the panic as usual.
    RestoreAndReport,
    /// Report as usual (the TUI does not own the terminal).
    Report,
    /// A background thread panicked while the TUI runs: leave the screen
    /// alone (the worker's result is reported as failed by the UI) and
    /// keep the message for after exit.
    Defer,
}

fn panic_action(on_main_thread: bool, terminal_active: bool) -> PanicAction {
    match (terminal_active, on_main_thread) {
        (false, _) => PanicAction::Report,
        (true, true) => PanicAction::RestoreAndReport,
        (true, false) => PanicAction::Defer,
    }
}

/// Install a panic hook that restores the terminal before the default panic
/// handler runs, so a panic in the event loop does not leave the user's
/// terminal stuck in raw mode with the alternate screen active.
///
/// A panic in a background thread (an import, a duplicate scan) does not
/// restore the terminal, since the event loop keeps running and the UI
/// reports the worker as failed; its message is printed by
/// [`report_worker_panics`] once the TUI has exited. Call this once, on the
/// main thread, before [`setup_terminal`].
pub fn install_panic_hook() {
    let main_thread = std::thread::current().id();
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let on_main = std::thread::current().id() == main_thread;
        match panic_action(on_main, TERMINAL_ACTIVE.load(Ordering::SeqCst)) {
            PanicAction::RestoreAndReport => {
                // Best-effort: undo every mode, ignoring errors, since the
                // guard may not get to run while unwinding.
                let mut control = Crossterm;
                for step in TerminalStep::ALL.iter().rev() {
                    let _ = control.disable(*step);
                }
                let _ = control.show_cursor();
                TERMINAL_ACTIVE.store(false, Ordering::SeqCst);
                previous_hook(info);
            }
            PanicAction::Report => previous_hook(info),
            PanicAction::Defer => {
                let name = std::thread::current()
                    .name()
                    .unwrap_or("background")
                    .to_string();
                if let Ok(mut panics) = WORKER_PANICS.lock() {
                    panics.push(format!("thread '{name}' panicked: {info}"));
                }
            }
        }
    }));
}

/// Print the panics of background threads collected while the TUI ran.
pub fn report_worker_panics() {
    if let Ok(panics) = WORKER_PANICS.lock() {
        for panic in panics.iter() {
            eprintln!("{panic}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    /// Records calls; fails `enable`/`disable` of the given steps.
    #[derive(Default)]
    struct Fake {
        calls: Vec<String>,
        fail_enable: Option<TerminalStep>,
        fail_disable: Vec<TerminalStep>,
        fail_cursor: bool,
    }

    impl TerminalControl for Fake {
        fn enable(&mut self, step: TerminalStep) -> io::Result<()> {
            self.calls.push(format!("on {step:?}"));
            if self.fail_enable == Some(step) {
                return Err(io::Error::other(format!("enable {step:?}")));
            }
            Ok(())
        }
        fn disable(&mut self, step: TerminalStep) -> io::Result<()> {
            self.calls.push(format!("off {step:?}"));
            if self.fail_disable.contains(&step) {
                return Err(io::Error::other(format!("disable {step:?}")));
            }
            Ok(())
        }
        fn show_cursor(&mut self) -> io::Result<()> {
            self.calls.push("cursor".into());
            if self.fail_cursor {
                return Err(io::Error::other("cursor"));
            }
            Ok(())
        }
    }

    /// A fake whose calls survive the guard being dropped.
    struct Shared(std::rc::Rc<std::cell::RefCell<Fake>>);

    impl TerminalControl for Shared {
        fn enable(&mut self, step: TerminalStep) -> io::Result<()> {
            self.0.borrow_mut().enable(step)
        }
        fn disable(&mut self, step: TerminalStep) -> io::Result<()> {
            self.0.borrow_mut().disable(step)
        }
        fn show_cursor(&mut self) -> io::Result<()> {
            self.0.borrow_mut().show_cursor()
        }
    }

    #[test]
    fn a_failed_setup_step_undoes_only_the_steps_before_it() {
        let fake = std::rc::Rc::new(std::cell::RefCell::new(Fake {
            fail_enable: Some(TerminalStep::MouseCapture),
            ..Fake::default()
        }));
        let error = TerminalGuard::enable(Shared(fake.clone()))
            .err()
            .expect("setup fails");
        assert_eq!(error.to_string(), "enable MouseCapture");
        assert_eq!(
            fake.borrow().calls,
            [
                "on RawMode",
                "on AlternateScreen",
                "on MouseCapture",
                "off AlternateScreen",
                "off RawMode",
                "cursor"
            ]
        );
    }

    #[test]
    fn restore_attempts_every_step_and_returns_the_first_error() {
        let mut guard = TerminalGuard::enable(Fake {
            fail_disable: vec![TerminalStep::BracketedPaste, TerminalStep::RawMode],
            fail_cursor: true,
            ..Fake::default()
        })
        .unwrap();
        let error = guard.restore().unwrap_err();
        assert_eq!(error.to_string(), "disable BracketedPaste");
        assert_eq!(
            guard.control().calls[4..],
            [
                "off BracketedPaste",
                "off MouseCapture",
                "off AlternateScreen",
                "off RawMode",
                "cursor"
            ]
        );
        // Already restored: a second restore (or the drop) changes nothing.
        let calls = guard.control().calls.len();
        assert!(guard.restore().is_ok());
        assert_eq!(guard.control().calls.len(), calls);
    }

    #[test]
    fn dropping_the_guard_restores_the_terminal() {
        let fake = std::rc::Rc::new(std::cell::RefCell::new(Fake::default()));
        drop(TerminalGuard::enable(Shared(fake.clone())).unwrap());
        assert_eq!(
            fake.borrow().calls[4..],
            [
                "off BracketedPaste",
                "off MouseCapture",
                "off AlternateScreen",
                "off RawMode",
                "cursor"
            ]
        );
    }

    #[test]
    fn the_event_loop_error_wins_over_a_cleanup_error() {
        let (result, cleanup) = finish(
            Err(anyhow::anyhow!("event loop failed")),
            Err(io::Error::other("tty gone")),
        );
        assert_eq!(result.unwrap_err().to_string(), "event loop failed");
        assert_eq!(cleanup.unwrap().to_string(), "tty gone");

        let (result, cleanup) = finish(Ok(()), Err(io::Error::other("tty gone")));
        assert_eq!(result.unwrap_err().to_string(), "restoring the terminal");
        assert!(cleanup.is_none());

        let (result, cleanup) = finish(Ok(()), Ok(()));
        assert!(result.is_ok() && cleanup.is_none());
    }

    #[test]
    fn only_a_main_thread_panic_restores_the_terminal() {
        assert_eq!(panic_action(true, true), PanicAction::RestoreAndReport);
        assert_eq!(panic_action(false, true), PanicAction::Defer);
        assert_eq!(panic_action(true, false), PanicAction::Report);
        assert_eq!(panic_action(false, false), PanicAction::Report);
    }
}
