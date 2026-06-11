//! Debounced file-system watcher for the `.bears/` directory.
//!
//! Call [`watch_bears_dir`] to start a background watcher. It returns a
//! [`tokio::sync::mpsc::Receiver<()>`] that emits a unit message whenever
//! any file in the watched directory is created, modified, or removed.
//!
//! The watcher runs on a dedicated OS thread (managed by `notify`) with a
//! 300 ms debounce window, bridged into async-land via a tokio task. The
//! caller can `select!` on the receiver alongside the crossterm event stream
//! to trigger live-reloads without polling.
//!
//! # Lifetime
//!
//! The returned [`Debouncer`] handle keeps the watcher alive. Drop it to stop
//! watching. The tokio receiver will be closed automatically once the watcher
//! thread exits.
//!
//! # Integration
//!
//! `watch_bears_dir` is called from the TUI event loop ([`super::run`]) to
//! enable live reload when `.bears/` changes on disk.

use std::path::Path;
use std::sync::mpsc as std_mpsc;
use std::time::Duration;

pub use notify_debouncer_mini::Debouncer;
use notify_debouncer_mini::new_debouncer;
use notify_debouncer_mini::notify::RecommendedWatcher;
use notify_debouncer_mini::notify::RecursiveMode;
use tokio::sync::mpsc as tokio_mpsc;

/// Debounce window applied to raw filesystem events.
const DEBOUNCE_TIMEOUT: Duration = Duration::from_millis(300);

/// Capacity of the async notification channel.
///
/// We only care "something changed"; a small buffer is fine.  If the consumer
/// falls behind we simply drop extra notifications rather than blocking.
const CHANNEL_CAPACITY: usize = 8;

/// Start a debounced watcher on `dir` and return:
///
/// * A [`Debouncer`] whose lifetime controls the watcher thread — drop it to stop.
/// * A [`tokio::sync::mpsc::Receiver<()>`] that yields `()` for every debounced
///   change batch detected in `dir`.
///
/// # Errors
///
/// Returns an error if the underlying `notify` watcher cannot be created or if
/// `dir` cannot be registered for watching.
pub fn watch_bears_dir(
    dir: &Path,
) -> Result<
    (Debouncer<RecommendedWatcher>, tokio_mpsc::Receiver<()>),
    notify_debouncer_mini::notify::Error,
> {
    // std channel: notify debouncer → bridge task
    let (std_tx, std_rx) = std_mpsc::channel();

    let mut debouncer = new_debouncer(DEBOUNCE_TIMEOUT, std_tx)?;
    debouncer.watcher().watch(dir, RecursiveMode::Recursive)?;

    // tokio channel: bridge task → TUI event loop
    let (tok_tx, tok_rx) = tokio_mpsc::channel(CHANNEL_CAPACITY);

    // Spawn a blocking tokio task that forwards std events to the tokio channel.
    // `spawn_blocking` keeps the tokio thread pool free for real async work.
    tokio::task::spawn_blocking(move || {
        for _result in &std_rx {
            // Ignore debounce errors; only forward "something changed".
            if tok_tx.blocking_send(()).is_err() {
                // Receiver dropped — TUI exited, stop the bridge.
                break;
            }
        }
    });

    Ok((debouncer, tok_rx))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tempfile::tempdir;

    /// Smoke test: verify that creating a file inside the watched dir eventually
    /// produces an event on the tokio receiver.
    ///
    /// This test is inherently timing-sensitive (filesystem events + debounce
    /// window). It uses a generous timeout to reduce flakiness in CI.  If it
    /// proves unreliable on a particular platform it can be marked `#[ignore]`.
    #[tokio::test]
    #[ignore = "timing-sensitive filesystem watcher; run with `cargo test -- --ignored`"]
    async fn watcher_emits_event_on_file_create() {
        let tmp = tempdir().expect("tempdir");
        let bears_dir = tmp.path().join(".bears");
        std::fs::create_dir_all(&bears_dir).expect("create .bears dir");

        let (_debouncer, mut rx) = watch_bears_dir(&bears_dir).expect("start watcher");

        // Give the watcher a moment to start before touching files.
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Write a file to trigger a filesystem event.
        std::fs::write(bears_dir.join("test-task.md"), "# hello\n").expect("write test file");

        // Wait up to 2 s for the debounced event (debounce window is 300 ms).
        let received = tokio::time::timeout(Duration::from_secs(2), rx.recv()).await;
        assert!(
            received.is_ok(),
            "timeout waiting for watcher event after file create"
        );
        assert_eq!(
            received.unwrap(),
            Some(()),
            "expected Some(()) from watcher channel"
        );
    }

    /// Verify that modifying an existing file triggers a reload signal, and that
    /// deleting a file also triggers a reload signal.
    ///
    /// Marked `#[ignore]` because it depends on real filesystem events and the
    /// debounce window.  Run with `cargo test -- --ignored` to exercise it.
    #[tokio::test]
    #[ignore = "timing-sensitive filesystem watcher; run with `cargo test -- --ignored`"]
    async fn watcher_emits_event_on_file_modify_and_delete() {
        let tmp = tempdir().expect("tempdir");
        let bears_dir = tmp.path().join(".bears");
        std::fs::create_dir_all(&bears_dir).expect("create .bears dir");

        // Pre-create the file *before* starting the watcher so the initial
        // create doesn't contribute a signal during our test.
        let task_file = bears_dir.join("abc-existing-task.md");
        std::fs::write(&task_file, "# initial\n").expect("write initial file");

        let (_debouncer, mut rx) = watch_bears_dir(&bears_dir).expect("start watcher");
        tokio::time::sleep(Duration::from_millis(100)).await;

        // --- modify ---
        std::fs::write(&task_file, "# modified\n").expect("modify file");

        let sig = tokio::time::timeout(Duration::from_secs(2), rx.recv()).await;
        assert!(
            sig.is_ok(),
            "timeout waiting for watcher event after file modify"
        );
        assert_eq!(sig.unwrap(), Some(()));

        // --- delete ---
        std::fs::remove_file(&task_file).expect("delete file");

        let sig = tokio::time::timeout(Duration::from_secs(2), rx.recv()).await;
        assert!(
            sig.is_ok(),
            "timeout waiting for watcher event after file delete"
        );
        assert_eq!(sig.unwrap(), Some(()));
    }

    /// Verify that a burst of rapid writes within the debounce window produces
    /// only a small number of signals (ideally one), not one per write.
    ///
    /// We write several files within a short window, then wait for the debounce
    /// to settle and count how many signals arrived.  The exact count depends on
    /// OS scheduling and inotify coalescing, so we only assert it is strictly
    /// less than the number of writes — not necessarily exactly one.
    ///
    /// Marked `#[ignore]` because it depends on real timing and debounce
    /// behaviour.  Run with `cargo test -- --ignored` to exercise it.
    #[tokio::test]
    #[ignore = "timing-sensitive debounce test; run with `cargo test -- --ignored`"]
    async fn watcher_debounce_coalesces_rapid_writes() {
        const WRITE_COUNT: usize = 10;

        let tmp = tempdir().expect("tempdir");
        let bears_dir = tmp.path().join(".bears");
        std::fs::create_dir_all(&bears_dir).expect("create .bears dir");

        let (_debouncer, mut rx) = watch_bears_dir(&bears_dir).expect("start watcher");
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Fire WRITE_COUNT writes in rapid succession — all within the debounce window.
        for i in 0..WRITE_COUNT {
            std::fs::write(
                bears_dir.join(format!("task-{i:02}.md")),
                format!("# task {i}\n"),
            )
            .expect("write task file");
        }

        // Drain all signals that arrive within (debounce + generous margin).
        // DEBOUNCE_TIMEOUT is 300 ms; we wait 1 s to let it fully settle.
        let drain_deadline = Duration::from_secs(1);
        let mut signal_count: usize = 0;
        let start = std::time::Instant::now();
        while start.elapsed() < drain_deadline {
            let remaining = drain_deadline.saturating_sub(start.elapsed());
            match tokio::time::timeout(remaining, rx.recv()).await {
                Ok(Some(())) => signal_count += 1,
                Ok(None) | Err(_) => break,
            }
        }

        assert!(
            signal_count > 0,
            "expected at least one signal after rapid writes"
        );
        assert!(
            signal_count < WRITE_COUNT,
            "expected debouncing to coalesce signals: got {signal_count} for {WRITE_COUNT} writes"
        );
    }
}
