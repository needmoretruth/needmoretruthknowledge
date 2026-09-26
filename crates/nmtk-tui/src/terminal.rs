//! Taking the terminal over, and giving it back exactly as it was found.
//!
//! nmtk puts the terminal in raw mode, switches to the alternate screen and hides the cursor. Each
//! of those has to be undone on every way out — a normal quit, an error, a panic, a signal — or
//! the reader is left at a shell that does not echo what they type, on a screen with no cursor.
//!
//! The drawing library offers an `init` that does the first half and installs a panic hook for the
//! second. It is not used here, for two reasons a reader would have met:
//!
//! - Its panic hook restores the terminal whichever thread panicked. A quest's worker thread that
//!   panics does not stop the program, so the screen kept drawing — onto the reader's ordinary
//!   screen, in cooked mode, with the panic message printed through the middle of it.
//! - A failure halfway through starting — raw mode on, alternate screen refused — returned an
//!   error with raw mode still on.
//!
//! And nothing handled being told to stop. `kill` ended the process in raw mode on the alternate
//! screen, with a quest's miners never told to stop; see [`stop_signal`].

use std::io::{self, Stdout, stdout};
use std::panic;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, Once, OnceLock};
use std::thread::{self, ThreadId};

use crossterm::cursor::Show;
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

/// The terminal nmtk draws on.
pub type Screen = Terminal<CrosstermBackend<Stdout>>;

/// Whether the terminal is ours right now, so it is given back once and only once.
static TAKEN: AtomicBool = AtomicBool::new(false);
/// The thread that draws. A panic anywhere else leaves the screen alone.
static DRAWING_THREAD: OnceLock<ThreadId> = OnceLock::new();
/// Panics from other threads, held until the terminal is given back so they can be read.
static DEFERRED: Mutex<Vec<String>> = Mutex::new(Vec::new());
static HOOK: Once = Once::new();

/// Takes the terminal over. On failure nothing is left half done.
pub fn take() -> io::Result<Screen> {
    install_panic_hook();
    let _ = DRAWING_THREAD.set(thread::current().id());
    TAKEN.store(true, Ordering::SeqCst);
    let taken = enable_raw_mode()
        .and_then(|()| execute!(stdout(), EnterAlternateScreen))
        .and_then(|()| Terminal::new(CrosstermBackend::new(stdout())));
    if taken.is_err() {
        let _ = give_back();
    }
    taken
}

/// Gives the terminal back: cooked mode, the ordinary screen, and a cursor. (Dropping the
/// terminal shows the cursor too; saying so here as well costs nothing and covers a path that
/// never reaches the drop.)
///
/// Every step is tried even when one before it fails, because a terminal with raw mode off and the
/// alternate screen still up is better than one with neither undone. Safe to call more than once;
/// only the first call does anything.
pub fn give_back() -> io::Result<()> {
    if !TAKEN.swap(false, Ordering::SeqCst) {
        return Ok(());
    }
    let raw = disable_raw_mode();
    let screen = execute!(stdout(), LeaveAlternateScreen, Show);
    raw.and(screen)
}

/// Panics from threads other than the drawing thread, oldest first, emptied as they are read.
pub fn deferred_panics() -> Vec<String> {
    std::mem::take(&mut *DEFERRED.lock().unwrap_or_else(|poisoned| poisoned.into_inner()))
}

/// A panic on the drawing thread gives the terminal back and then says what happened, so the
/// message lands on the reader's own screen rather than inside the alternate one, which vanishes
/// with it. A panic on any other thread is written down and said after the program ends.
fn install_panic_hook() {
    HOOK.call_once(|| {
        let previous = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            let here = thread::current();
            let drawing = DRAWING_THREAD.get().is_none_or(|id| *id == here.id());
            if drawing || !TAKEN.load(Ordering::SeqCst) {
                let _ = give_back();
                for message in deferred_panics() {
                    eprintln!("{message}");
                }
                previous(info);
            } else {
                let name = here.name().unwrap_or("<unnamed>");
                DEFERRED
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .push(format!("nmtk: thread '{name}' {info}"));
            }
        }));
    });
}

/// Asks to be told when the program is told to stop from outside: `kill`, a closed terminal, or an
/// interrupt sent while raw mode is off.
///
/// Left to their defaults these signals end the process on the spot, with the terminal still in
/// raw mode and the quest's threads never stopped. Caught, they set a flag the event loop reads
/// every frame, and the program leaves the way `q` does. A second signal while the first is being
/// handled ends the program at once, so a quest that will not stop cannot hold it open.
pub fn stop_signal() -> std::sync::Arc<AtomicBool> {
    let flag = std::sync::Arc::new(AtomicBool::new(false));
    #[cfg(unix)]
    {
        use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
        for signal in [SIGTERM, SIGHUP, SIGINT] {
            // Order matters: the shutdown on a second signal is registered before the flag, so
            // it sees the flag as it was before this signal set it.
            let _ = signal_hook::flag::register_conditional_shutdown(signal, 1, flag.clone());
            let _ = signal_hook::flag::register(signal, flag.clone());
        }
    }
    flag
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A worker's panic is held for later, not printed through the middle of the screen and not
    /// answered by tearing the screen down under the drawing thread.
    ///
    /// One test rather than two, because both halves use the process-wide state the program has
    /// one of, and two tests running at once would each see the other's.
    #[test]
    fn a_panic_off_the_drawing_thread_is_kept_for_after() {
        // A terminal that was never taken is never "given back": nothing is written to a stdout
        // that is not a terminal, and nothing fails.
        assert!(give_back().is_ok());
        assert!(give_back().is_ok());

        install_panic_hook();
        // Pretend the terminal is ours and that some other thread draws.
        let _ = DRAWING_THREAD.set(thread::current().id());
        TAKEN.store(true, Ordering::SeqCst);
        let worker = thread::Builder::new()
            .name("miner".to_string())
            .spawn(|| panic!("the engine gave up"))
            .expect("a thread");
        assert!(worker.join().is_err());
        let still_taken = TAKEN.swap(false, Ordering::SeqCst);
        assert!(still_taken, "a worker's panic gave the terminal back under the drawing thread");
        let kept = deferred_panics();
        assert!(
            kept.iter().any(|m| m.contains("'miner'") && m.contains("the engine gave up")),
            "the worker's panic was not kept: {kept:?}"
        );
        assert!(deferred_panics().is_empty(), "read twice");
    }
}
