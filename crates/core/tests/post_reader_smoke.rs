//! Live, read-only reader check. Never comments, reacts or sends.
use blindfb_core::fb::{
    feed_args, notifs_args, parse_notifications, parse_records, recent_feed_args, web_url,
};
use blindfb_core::posts;
use blindfb_core::Fb;

#[test]
#[ignore = "opens a signed-in post and adds a local-only form fixture; never submits to Facebook"]
fn participation_reads_real_native_dom_controls() {
    let fb = Fb::discover().expect("install the backend first");
    let page = "blindfb-questions-smoke";
    let url = "https://www.facebook.com/groups/271579756259079/posts/29077339271923077";
    posts::load(&fb, page, url).expect("read the post");
    let script = r#"(() => {
      const d = document.createElement('div');
      d.setAttribute('role', 'dialog');
      d.style.cssText = 'position:fixed;inset:0;background:white;z-index:2147483647';
      d.innerHTML = `<div aria-disabled="false"><span>Do you live in Hope?</span>
        <input type="radio" name="fixture" aria-labelledby="blindfb-fixture-yes"><label id="blindfb-fixture-yes">yes</label>
        <input type="radio" name="fixture" aria-labelledby="blindfb-fixture-no"><label id="blindfb-fixture-no">no</label></div>
        <div aria-disabled="false"><label><span aria-label="Will your content be about Hope?">Write an answer...<textarea maxlength="250"></textarea></span></label></div>
        <label><input type="checkbox" name="agree-to-group-rules">I agree to the group rules</label>
        <p>Group rules: Be respectful.</p>`;
      document.body.append(d);
      try { return window.__blindfbQuestions.read(d); }
      finally { d.remove(); }
    })()"#;
    let result = fb.run(&["page", page, "eval", script]);
    let _ = posts::close(&fb, page);
    let form = posts::parse_participation(&result.expect("read the local form"))
        .expect("parse form").expect("participation form");
    assert_eq!(form.fields.len(), 3);
    assert_eq!(form.fields[0].prompt, "Do you live in Hope?");
    assert_eq!(form.fields[0].choices, ["yes", "no"]);
    assert_eq!(form.fields[1].prompt, "Will your content be about Hope?");
    assert_eq!(form.fields[2].kind, "check");
    assert!(form.text.contains("Be respectful"));
}

#[test]
#[ignore = "reads a signed-in group post; closes only its own test tab"]
fn reader_recovers_a_lost_page_without_restarting_the_daemon() {
    let fb = Fb::discover().expect("install the backend first");
    let page = "blindfb-recovery-smoke";
    let url = "https://www.facebook.com/groups/271579756259079/posts/29077339271923077";
    posts::load(&fb, page, url).expect("read the post");
    // Lose just the named test page, preserving the app's browser and all other readers.
    fb.run(&["page", page, "close"]).expect("close the test page");
    let result = posts::refresh(&fb, page).and_then(|json| posts::parse_detail(&json));
    let _ = posts::close(&fb, page);
    let detail = result.expect("recover the lost page");
    assert!(!detail.text.is_empty());
    assert!(detail.url.contains("/groups/271579756259079/posts/29077339271923077"));
}

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
