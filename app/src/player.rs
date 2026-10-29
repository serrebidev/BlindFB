//! A native, non-focusable video surface with playback controlled by the reader.
use wxdragon::prelude::*;

#[derive(Debug)]
pub enum PlayerEvent {
    Loaded,
    StateChanged,
    Finished,
    Error(String),
}

#[cfg(not(target_os = "windows"))]
mod platform {
    use super::*;
    use std::{cell::RefCell, rc::Rc};
    use wxdragon::widgets::media_ctrl::SeekMode;

    #[derive(Clone)]
    pub struct Player {
        media: MediaCtrl,
        events: Rc<RefCell<Vec<PlayerEvent>>>,
    }

    impl Player {
        pub fn new(parent: &impl WxWidget) -> Self {
            let media = MediaCtrl::builder(parent).with_size(Size::new(320, 120)).build();
            media.set_name("Video display");
            media.disable_focus_from_keyboard();
            media.set_can_focus(false);
            media.show_player_controls(MediaCtrlPlayerControls::None);
            let events = Rc::new(RefCell::new(Vec::new()));
            let queue = events.clone();
            media.on_loaded(move |_| queue.borrow_mut().push(PlayerEvent::Loaded));
            let queue = events.clone();
            media.on_state_changed(move |_| queue.borrow_mut().push(PlayerEvent::StateChanged));
            let queue = events.clone();
            media.on_finished(move |_| queue.borrow_mut().push(PlayerEvent::Finished));
            Self { media, events }
        }

        pub fn surface(&self) -> MediaCtrl {
            self.media
        }
        pub fn load_uri(&self, uri: &str) -> bool {
            let loaded = !uri.contains('\0') && self.media.load_uri(uri);
            if !loaded {
                self.events.borrow_mut().push(PlayerEvent::Error("The media backend could not load this video.".to_owned()));
            }
            loaded
        }
        pub fn play(&self) -> bool {
            self.media.play()
        }
        pub fn pause(&self) -> bool {
            self.media.pause()
        }
        pub fn stop(&self) -> bool {
            self.media.stop()
        }
        pub fn set_volume(&self, volume: f64) -> bool {
            self.media.set_volume(volume)
        }
        pub fn get_state(&self) -> MediaState {
            self.media.get_state()
        }
        pub fn tell(&self) -> i64 {
            self.media.tell()
        }
        pub fn length(&self) -> i64 {
            self.media.length()
        }
        pub fn seek(&self, position: i64) -> i64 {
            self.media.seek(position, SeekMode::FromStart)
        }
        pub fn poll(&self) -> Vec<PlayerEvent> {
            std::mem::take(&mut *self.events.borrow_mut())
        }
        pub fn shutdown(&self) {
            self.media.stop();
        }
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use super::*;
    use serde_json::{Value, json};
    use std::{
        cell::RefCell,
        ffi::c_void,
        fs::{File, OpenOptions},
        io::{Read, Write},
        os::windows::{io::AsRawHandle, process::CommandExt},
        process::{Command, Stdio},
        rc::Rc,
        sync::{
            Arc, Mutex,
            atomic::{AtomicU64, Ordering},
            mpsc::{self, Receiver, SyncSender},
        },
        thread,
        time::{Duration, Instant},
    };

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn PeekNamedPipe(
            pipe: *mut c_void,
            buffer: *mut c_void,
            size: u32,
            read: *mut u32,
            available: *mut u32,
            left: *mut u32,
        ) -> i32;
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut c_void;
        fn TerminateProcess(process: *mut c_void, code: u32) -> i32;
        fn CloseHandle(handle: *mut c_void) -> i32;
    }

    #[derive(Clone, Copy)]
    struct Snapshot {
        state: MediaState,
        position: i64,
        duration: i64,
        paused: bool,
        idle: bool,
        stopped: bool,
    }

    impl Default for Snapshot {
        fn default() -> Self {
            Self {
                state: MediaState::Stopped,
                position: 0,
                duration: 0,
                paused: true,
                idle: true,
                stopped: true,
            }
        }
    }

    impl Snapshot {
        fn update_state(&mut self) -> Option<PlayerEvent> {
            let state = if self.idle || self.stopped {
                MediaState::Stopped
            } else if self.paused {
                MediaState::Paused
            } else {
                MediaState::Playing
            };
            if self.state == state {
                return None;
            }
            self.state = state;
            Some(PlayerEvent::StateChanged)
        }

        fn parse(&mut self, message: &Value) -> Option<PlayerEvent> {
            if message.get("request_id").is_some() {
                return match message["error"].as_str() {
                    Some("success") | None => None,
                    Some(_) => Some(PlayerEvent::Error(
                        "The video player could not complete that action.".into(),
                    )),
                };
            }
            match message["event"].as_str()? {
                "file-loaded" => {
                    self.idle = false;
                    self.stopped = false;
                    self.state = if self.paused { MediaState::Paused } else { MediaState::Playing };
                    Some(PlayerEvent::Loaded)
                }
                "property-change" => {
                    match message["name"].as_str()? {
                        "time-pos" => self.position = milliseconds(&message["data"]),
                        "duration" => self.duration = milliseconds(&message["data"]),
                        "pause" => self.paused = message["data"].as_bool().unwrap_or(true),
                        "idle-active" => self.idle = message["data"].as_bool().unwrap_or(true),
                        _ => return None,
                    }
                    self.update_state()
                }
                "end-file" => match message["reason"].as_str() {
                    Some("eof") => {
                        self.stopped = true;
                        self.state = MediaState::Stopped;
                        Some(PlayerEvent::Finished)
                    }
                    Some("error") => {
                        self.stopped = true;
                        self.state = MediaState::Stopped;
                        Some(PlayerEvent::Error("The video could not be played. Try another video or check your connection.".into()))
                    }
                    _ => None,
                },
                _ => None,
            }
        }
    }

    fn milliseconds(value: &Value) -> i64 {
        value.as_f64().filter(|v| v.is_finite()).map(|v| (v.max(0.0) * 1000.0) as i64).unwrap_or(0)
    }

    enum Action {
        Load(String),
        Play,
        Pause,
        Stop,
        Volume(f64),
        Seek(i64),
    }
    struct Update {
        snapshot: Snapshot,
        event: Option<PlayerEvent>,
    }
    #[derive(Default)]
    struct Lifetime {
        stopped: bool,
        pid: u32,
    }

    impl Lifetime {
        fn shutdown(&mut self) {
            self.stopped = true;
            if self.pid != 0 {
                // Terminate before the native parent is destroyed; only the worker waits/reaps.
                unsafe {
                    let handle = OpenProcess(1, 0, self.pid);
                    if !handle.is_null() {
                        TerminateProcess(handle, 0);
                        CloseHandle(handle);
                    }
                }
            }
        }
    }

    struct Inner {
        surface: Panel,
        actions: SyncSender<Action>,
        updates: Receiver<Update>,
        snapshot: RefCell<Snapshot>,
        lifetime: Arc<Mutex<Lifetime>>,
    }

    impl Drop for Inner {
        fn drop(&mut self) {
            self.lifetime.lock().unwrap().shutdown();
        }
    }

    #[derive(Clone)]
    pub struct Player(Rc<Inner>);

    impl Player {
        pub fn new(parent: &impl WxWidget) -> Self {
            let surface = Panel::builder(parent)
                .with_style(PanelStyle::BorderNone)
                .with_size(Size::new(320, 120))
                .build();
            surface.set_name("Video display");
            surface.disable_focus_from_keyboard();
            surface.set_can_focus(false);
            // mpv creates a child window; disabling the display also prevents mouse focus there.
            surface.enable(false);
            let hwnd = surface.get_handle() as usize as u32;
            let (actions, receive) = mpsc::sync_channel(32);
            let (send, updates) = mpsc::sync_channel(256);
            let lifetime = Arc::new(Mutex::new(Lifetime::default()));
            let worker_lifetime = lifetime.clone();
            thread::spawn(move || worker(hwnd, receive, send, worker_lifetime));
            Self(Rc::new(Inner {
                surface,
                actions,
                updates,
                snapshot: RefCell::new(Snapshot::default()),
                lifetime,
            }))
        }

        pub fn surface(&self) -> Panel {
            self.0.surface
        }
        fn send(&self, action: Action) -> bool {
            self.0.surface.is_valid() && self.0.actions.try_send(action).is_ok()
        }
        pub fn load_uri(&self, uri: &str) -> bool {
            if uri.is_empty()
                || uri.contains('\0')
                || !blindfb_core::mpv::managed_bin().is_some_and(|path| path.is_file())
            {
                return false;
            }
            self.send(Action::Load(uri.into()))
        }
        pub fn play(&self) -> bool {
            self.send(Action::Play)
        }
        pub fn pause(&self) -> bool {
            self.send(Action::Pause)
        }
        pub fn stop(&self) -> bool {
            self.send(Action::Stop)
        }
        pub fn set_volume(&self, volume: f64) -> bool {
            volume.is_finite() && self.send(Action::Volume(volume.clamp(0.0, 1.0)))
        }
        pub fn get_state(&self) -> MediaState {
            self.0.snapshot.borrow().state
        }
        pub fn tell(&self) -> i64 {
            self.0.snapshot.borrow().position
        }
        pub fn length(&self) -> i64 {
            self.0.snapshot.borrow().duration
        }
        pub fn seek(&self, position: i64) -> i64 {
            if self.send(Action::Seek(position.max(0))) { position.max(0) } else { -1 }
        }
        pub fn poll(&self) -> Vec<PlayerEvent> {
            let mut events = Vec::new();
            while let Ok(update) = self.0.updates.try_recv() {
                *self.0.snapshot.borrow_mut() = update.snapshot;
                if let Some(event) = update.event {
                    events.push(event);
                }
            }
            events
        }
        pub fn shutdown(&self) {
            self.0.lifetime.lock().unwrap().shutdown();
        }
    }

    fn send_command(pipe: &mut File, serial: &mut u64, command: Value) -> std::io::Result<()> {
        *serial += 1;
        let mut bytes = serde_json::to_vec(&json!({"command": command, "request_id": *serial}))?;
        bytes.push(b'\n');
        pipe.write_all(&bytes)
    }

    fn worker(
        hwnd: u32,
        actions: Receiver<Action>,
        updates: SyncSender<Update>,
        lifetime: Arc<Mutex<Lifetime>>,
    ) {
        let mut snapshot = Snapshot::default();
        let report = |snapshot, event| {
            let update = Update { snapshot, event };
            if update.event.is_some() {
                // Apply backpressure only on the worker; loaded/finished/errors must not disappear.
                let _ = updates.send(update);
            } else {
                let _ = updates.try_send(update);
            }
        };
        // Wait for the first load: installation and ordinary text posts need no process.
        let first = loop {
            if lifetime.lock().unwrap().stopped {
                return;
            }
            match actions.recv_timeout(Duration::from_millis(100)) {
                Ok(action @ Action::Load(_)) => break action,
                Ok(_) | Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }
        };
        let Some(binary) = blindfb_core::mpv::managed_bin() else {
            report(snapshot, Some(PlayerEvent::Error("The video player is not installed.".into())));
            return;
        };
        static SERIAL: AtomicU64 = AtomicU64::new(0);
        let pipe_name = format!(
            r"\\.\pipe\blindfb-mpv-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        );
        let mut guard = lifetime.lock().unwrap();
        if guard.stopped {
            return;
        }
        let child = Command::new(binary)
            .args([
                "--no-config",
                "--idle=yes",
                "--force-window=yes",
                "--pause=yes",
                "--osc=no",
                "--osd-level=0",
                "--input-default-bindings=no",
                "--input-vo-keyboard=no",
                "--input-cursor=no",
                "--cursor-autohide=no",
                "--terminal=no",
                "--msg-level=all=no",
                "--ytdl=no",
                "--load-scripts=no",
                &format!("--wid={hwnd}"),
                &format!("--input-ipc-server={pipe_name}"),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(0x08000000)
            .spawn();
        let mut child = match child {
            Ok(child) => child,
            Err(_) => {
                drop(guard);
                report(
                    snapshot,
                    Some(PlayerEvent::Error("The video player could not start.".into())),
                );
                return;
            }
        };
        guard.pid = child.id();
        drop(guard);
        let result = (|| -> std::io::Result<()> {
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut pipe = loop {
                if lifetime.lock().unwrap().stopped {
                    return Ok(());
                }
                match OpenOptions::new().read(true).write(true).open(&pipe_name) {
                    Ok(pipe) => break pipe,
                    Err(error) => {
                        if Instant::now() >= deadline || child.try_wait()?.is_some() {
                            return Err(error);
                        }
                        thread::sleep(Duration::from_millis(20));
                    }
                }
            };
            let mut serial = 0;
            for (id, name) in ["time-pos", "duration", "pause", "idle-active"].iter().enumerate() {
                send_command(&mut pipe, &mut serial, json!(["observe_property", id, name]))?;
            }
            let mut pending = Some(first);
            let mut url = String::new();
            let mut buffer = Vec::new();
            loop {
                if lifetime.lock().unwrap().stopped {
                    return Ok(());
                }
                while let Some(action) = pending.take().or_else(|| actions.try_recv().ok()) {
                    let previous = snapshot.state;
                    match action {
                        Action::Load(uri) => {
                            url = uri;
                            snapshot = Snapshot::default();
                            send_command(
                                &mut pipe,
                                &mut serial,
                                json!(["set_property", "pause", true]),
                            )?;
                            send_command(
                                &mut pipe,
                                &mut serial,
                                json!(["loadfile", url, "replace"]),
                            )?;
                        }
                        Action::Play => {
                            if snapshot.idle && !url.is_empty() {
                                send_command(
                                    &mut pipe,
                                    &mut serial,
                                    json!(["loadfile", url, "replace"]),
                                )?;
                            }
                            snapshot.stopped = false;
                            snapshot.paused = false;
                            send_command(
                                &mut pipe,
                                &mut serial,
                                json!(["set_property", "pause", false]),
                            )?;
                            snapshot.update_state();
                        }
                        Action::Pause => {
                            send_command(
                                &mut pipe,
                                &mut serial,
                                json!(["set_property", "pause", true]),
                            )?;
                        }
                        Action::Stop => {
                            snapshot.stopped = true;
                            snapshot.position = 0;
                            snapshot.state = MediaState::Stopped;
                            send_command(
                                &mut pipe,
                                &mut serial,
                                json!(["set_property", "pause", true]),
                            )?;
                            if !snapshot.idle {
                                send_command(
                                    &mut pipe,
                                    &mut serial,
                                    json!(["seek", 0, "absolute+exact"]),
                                )?;
                            }
                        }
                        Action::Seek(position) => {
                            send_command(
                                &mut pipe,
                                &mut serial,
                                json!(["seek", position as f64 / 1000.0, "absolute+exact"]),
                            )?;
                        }
                        Action::Volume(volume) => {
                            send_command(
                                &mut pipe,
                                &mut serial,
                                json!(["set_property", "volume", volume * 100.0]),
                            )?;
                        }
                    }
                    report(
                        snapshot,
                        (snapshot.state != previous).then_some(PlayerEvent::StateChanged),
                    );
                }
                let mut available = 0;
                let ok = unsafe {
                    PeekNamedPipe(
                        pipe.as_raw_handle(),
                        std::ptr::null_mut(),
                        0,
                        std::ptr::null_mut(),
                        &mut available,
                        std::ptr::null_mut(),
                    )
                };
                if ok == 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if available > 0 {
                    let mut chunk = [0u8; 8192];
                    let count = pipe.read(&mut chunk[..(available as usize).min(8192)])?;
                    if count == 0 {
                        return Err(std::io::ErrorKind::UnexpectedEof.into());
                    }
                    buffer.extend_from_slice(&chunk[..count]);
                    if buffer.len() > 65536 {
                        return Err(std::io::ErrorKind::InvalidData.into());
                    }
                    while let Some(end) = buffer.iter().position(|byte| *byte == b'\n') {
                        if let Ok(message) = serde_json::from_slice::<Value>(&buffer[..end]) {
                            let event = snapshot.parse(&message);
                            report(snapshot, event);
                        }
                        buffer.drain(..=end);
                    }
                }
                if child.try_wait()?.is_some() {
                    return Err(std::io::ErrorKind::BrokenPipe.into());
                }
                thread::sleep(Duration::from_millis(20));
            }
        })();
        if result.is_err() && !lifetime.lock().unwrap().stopped {
            snapshot.state = MediaState::Stopped;
            report(
                snapshot,
                Some(PlayerEvent::Error(
                    "The video player disconnected. Close this post and try again.".into(),
                )),
            );
        }
        let _ = child.kill();
        let _ = child.wait();
        lifetime.lock().unwrap().pid = 0;
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn observed_state_and_time_are_cached_without_repeated_state_events() {
            let mut state = Snapshot::default();
            assert!(matches!(
                state.parse(&json!({"event":"file-loaded"})),
                Some(PlayerEvent::Loaded)
            ));
            assert_eq!(state.state, MediaState::Paused);
            assert!(matches!(
                state.parse(&json!({"event":"property-change","name":"pause","data":false})),
                Some(PlayerEvent::StateChanged)
            ));
            assert_eq!(state.state, MediaState::Playing);
            assert!(
                state
                    .parse(&json!({"event":"property-change","name":"time-pos","data":1.25}))
                    .is_none()
            );
            assert_eq!(state.position, 1250);
            assert!(
                state
                    .parse(&json!({"event":"property-change","name":"pause","data":false}))
                    .is_none()
            );
            assert!(matches!(
                state.parse(&json!({"event":"end-file","reason":"eof"})),
                Some(PlayerEvent::Finished)
            ));
            assert_eq!(state.state, MediaState::Stopped);
        }

        #[test]
        fn command_errors_and_load_errors_are_reported_without_private_urls() {
            let mut state = Snapshot::default();
            assert!(state.parse(&json!({"request_id":1,"error":"success"})).is_none());
            assert!(matches!(
                state.parse(&json!({"request_id":2,"error":"invalid parameter"})),
                Some(PlayerEvent::Error(_))
            ));
            let Some(PlayerEvent::Error(text)) = state.parse(&json!({"event":"end-file","reason":"error","file_error":"https://private.invalid/token"})) else { panic!("missing load error") };
            assert!(!text.contains("private.invalid"));
            assert_eq!(milliseconds(&Value::Null), 0);
            assert_eq!(milliseconds(&json!(-5)), 0);
        }
    }
}

pub use platform::Player;
