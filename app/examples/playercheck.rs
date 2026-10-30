//! Read-only native playback check: cargo run -p blindfb --example playercheck -- <Facebook URL>
use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};
use wxdragon::prelude::*;
#[path = "../src/player.rs"]
mod player;
use player::{Player, PlayerEvent};

fn main() {
    let target = std::env::args().nth(1).expect("provide a Facebook video or reel URL");
    let fb = blindfb_core::Fb::discover().expect("install the Facebook backend first");
    let page = format!("blindfb-playercheck-{}", std::process::id());
    let detail = blindfb_core::posts::load(&fb, &page, &target).expect("read video");
    let detail = blindfb_core::posts::parse_detail(&detail).expect("parse video");
    assert!(detail.video.is_some(), "reader must expose a video");
    let stream = blindfb_core::media::stream_url(&fb, &page, &target);
    let _ = blindfb_core::posts::close(&fb, &page);
    let stream = stream.expect("resolve video");
    #[cfg(windows)]
    blindfb_core::mpv::ensure().expect("install the managed player");
    let passed = Rc::new(Cell::new(false));
    let result = passed.clone();
    let _ = wxdragon::main(move |_| {
        let frame = Frame::builder()
            .with_title("BlindFB playback check")
            .with_size(Size::new(480, 320))
            .build();
        let media = Player::new(&frame);
        let layout = BoxSizer::builder(Orientation::Vertical).build();
        layout.add(&media.surface(), 1, SizerFlag::Expand, 0);
        frame.set_sizer(layout, true);
        let loaded = Rc::new(Cell::new(false));
        let phase = Cell::new(0);
        let start = Instant::now();
        let timer = Box::leak(Box::new(Timer::new(&frame)));
        let ticking: &'static Timer<Frame> = timer;
        let checked_media = media.clone();
        timer.on_tick(move |_| {
            for event in checked_media.poll() {
                match event {
                    PlayerEvent::Loaded => {
                        assert!(checked_media.set_volume(0.5));
                        loaded.set(checked_media.play());
                    }
                    PlayerEvent::Error(error) => eprintln!("Player error: {error}"),
                    PlayerEvent::CommandFailed(error) => {
                        eprintln!("Player command failed: {error}")
                    }
                    _ => {}
                }
            }
            if loaded.get() && phase.get() == 0 && checked_media.tell() >= 1000 {
                assert!(checked_media.pause());
                phase.set(1);
            } else if phase.get() == 1 && checked_media.get_state() == MediaState::Paused {
                assert!(checked_media.seek(2000) >= 0);
                assert!(checked_media.set_volume(1.0));
                assert!(checked_media.play());
                phase.set(2);
            } else if phase.get() == 2 && checked_media.tell() >= 3000 {
                assert!(checked_media.stop());
                phase.set(3);
            } else if phase.get() == 3 && checked_media.get_state() == MediaState::Stopped {
                assert!(checked_media.play());
                phase.set(4);
            } else if phase.get() == 4 && checked_media.tell() >= 1000 && checked_media.get_state() == MediaState::Playing {
                result.set(true);
                println!("PASS: native controls player advanced; duration {} ms", checked_media.length());
                ticking.stop();
                checked_media.shutdown();
                frame.close(true);
            } else if start.elapsed() > Duration::from_secs(30) {
                eprintln!(
                    "FAIL: native player did not advance; loaded={}, state={:?}",
                    loaded.get(),
                    checked_media.get_state()
                );
                ticking.stop();
                checked_media.shutdown();
                frame.close(true);
            }
        });
        let closing_media = media.clone();
        frame.on_destroy(move |_| { ticking.stop(); closing_media.shutdown(); });
        frame.show(true);
        assert!(media.load_uri(&stream), "native player rejected stream");
        timer.start(250, false);
    });
    assert!(passed.get(), "native playback did not advance");
}
