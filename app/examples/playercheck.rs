//! Read-only native playback check: cargo run -p blindfb --example playercheck -- <Facebook URL>
use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};
use wxdragon::prelude::*;

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
    let passed = Rc::new(Cell::new(false));
    let result = passed.clone();
    let _ = wxdragon::main(move |_| {
        let frame = Frame::builder()
            .with_title("BlindFB playback check")
            .with_size(Size::new(480, 320))
            .build();
        let builder = MediaCtrl::builder(&frame).with_size(Size::new(480, 270));
        #[cfg(windows)]
        let builder = builder.with_backend_name(MediaCtrl::BACKEND_WMP10);
        let media = builder.build();
        media.set_name("Video display. Playback check closes automatically");
        let loaded = Rc::new(Cell::new(false));
        let ready = loaded.clone();
        media.on_loaded(move |_| {
            ready.set(media.play());
        });
        let start = Instant::now();
        let timer = Box::leak(Box::new(Timer::new(&frame)));
        let ticking: &'static Timer<Frame> = timer;
        timer.on_tick(move |_| {
            if loaded.get() && media.tell() >= 1000 && media.get_state() == MediaState::Playing {
                result.set(true);
                println!("PASS: native player advanced; duration {} ms", media.length());
                ticking.stop();
                media.pause();
                media.stop();
                frame.close(true);
            } else if start.elapsed() > Duration::from_secs(30) {
                eprintln!(
                    "FAIL: native player did not advance; loaded={}, state={:?}",
                    loaded.get(),
                    media.get_state()
                );
                ticking.stop();
                frame.close(true);
            }
        });
        frame.show(true);
        assert!(media.load_uri(&stream), "native player rejected stream");
        timer.start(250, false);
    });
    assert!(passed.get(), "native playback did not advance");
}
