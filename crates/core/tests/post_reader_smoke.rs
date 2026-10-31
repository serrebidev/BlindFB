//! Live, read-only reader check. Never comments, reacts or sends.
use blindfb_core::fb::{
    feed_args, notifs_args, parse_notifications, parse_records, recent_feed_args, web_url,
};
use blindfb_core::posts;
use blindfb_core::Fb;

#[test]
#[ignore = "reads the signed-in Facebook account through the dedicated browser"]
fn reads_notifications_feeds_and_posts() {
    let fb = Fb::discover().expect("install the backend first");
    let run = |args: Vec<String>| {
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        fb.run(&args)
    };
    let notes = parse_notifications(&run(notifs_args(10)).unwrap()).unwrap();
    let mut targets: Vec<String> = notes.iter().map(|r| web_url(r.field(3))).collect();
    for args in [recent_feed_args(8), feed_args(8, false, true, true)] {
        let rows = parse_records(&run(args).unwrap());
        targets.extend(
            rows.iter()
                .filter(|r| r.field(0) != "-")
                .map(|r| web_url(r.field(0))),
        );
    }
    let page = "blindfb-smoke";
    let mut read = 0;
    for url in targets.iter().filter_map(|u| posts::reader_url(u)) {
        let result = posts::load(&fb, page, &url)
            .and_then(|_| posts::refresh(&fb, page))
            .and_then(|json| posts::parse_detail(&json));
        let _ = posts::close(&fb, page);
        match result {
            Ok(d) => {
                read += 1;
                println!(
                    "OK {url}: {:?}, {} chars, {} comments",
                    d.author,
                    d.text.chars().count(),
                    d.comments.len()
                );
            }
            Err(e) => println!("FAIL {url}: {e}"),
        }
    }
    for url in targets.iter().filter(|u| posts::reader_url(u).is_none()) {
        println!("SKIP {url}");
    }
    assert!(read > 0, "no post could be read");
}
