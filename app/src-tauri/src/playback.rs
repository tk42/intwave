//! Playback runtime. cpal streams are `!Send`, so the `Player` lives on a
//! dedicated audio thread; the GUI controls it by message and reads the playhead
//! + state from shared atomics. This keeps the Tauri-managed `AppState` `Send`.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use intwav_engine::ScratchReader;
use intwav_playback::{Player, PreviewChain};

/// Commands sent to the audio thread.
pub enum PlayCmd {
    Load {
        scratch_path: PathBuf,
        chain: PreviewChain,
    },
    Play,
    Pause,
    Stop,
    Seek(u64),
    Quit,
}

/// Handle held in `AppState`: a control channel + shared readouts.
pub struct PlaybackHandle {
    tx: Sender<PlayCmd>,
    position: Arc<AtomicU64>,
    playing: Arc<AtomicBool>,
    error: Arc<Mutex<Option<String>>>,
}

impl PlaybackHandle {
    pub fn send(&self, cmd: PlayCmd) {
        let _ = self.tx.send(cmd);
    }
    pub fn position(&self) -> u64 {
        self.position.load(Ordering::Relaxed)
    }
    pub fn playing(&self) -> bool {
        self.playing.load(Ordering::Relaxed)
    }
    pub fn take_error(&self) -> Option<String> {
        self.error.lock().unwrap().take()
    }
}

impl Drop for PlaybackHandle {
    fn drop(&mut self) {
        let _ = self.tx.send(PlayCmd::Quit);
    }
}

/// Spawn the audio thread and return its control handle.
pub fn spawn() -> PlaybackHandle {
    let (tx, rx) = channel::<PlayCmd>();
    let position = Arc::new(AtomicU64::new(0));
    let playing = Arc::new(AtomicBool::new(false));
    let error = Arc::new(Mutex::new(None));
    let (pos, play, err) = (position.clone(), playing.clone(), error.clone());

    thread::spawn(move || {
        let mut player: Option<Player<ScratchReader>> = None;
        loop {
            match rx.recv_timeout(Duration::from_millis(40)) {
                Ok(PlayCmd::Load {
                    scratch_path,
                    chain,
                }) => {
                    *err.lock().unwrap() = None;
                    player = None; // dropping the old Player stops its stream
                    match ScratchReader::open(&scratch_path).map_err(|e| e.to_string()) {
                        Ok(reader) => match Player::new(reader, chain) {
                            Ok(p) => player = Some(p),
                            Err(e) => *err.lock().unwrap() = Some(e.to_string()),
                        },
                        Err(e) => *err.lock().unwrap() = Some(e),
                    }
                }
                Ok(PlayCmd::Play) => {
                    if let Some(p) = &player {
                        p.play();
                        play.store(true, Ordering::Relaxed);
                    }
                }
                Ok(PlayCmd::Pause) => {
                    if let Some(p) = &player {
                        p.pause();
                    }
                    play.store(false, Ordering::Relaxed);
                }
                Ok(PlayCmd::Stop) => {
                    if let Some(p) = &player {
                        p.stop();
                    }
                    play.store(false, Ordering::Relaxed);
                }
                Ok(PlayCmd::Seek(frame)) => {
                    if let Some(p) = &player {
                        p.seek(frame);
                    }
                }
                Ok(PlayCmd::Quit) | Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => {}
            }
            if let Some(p) = &player {
                pos.store(p.position(), Ordering::Relaxed);
            }
        }
    });

    PlaybackHandle {
        tx,
        position,
        playing,
        error,
    }
}
