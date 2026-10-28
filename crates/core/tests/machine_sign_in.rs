//! Opt-in checks against the installed backend and browsers; never print account data.

use blindfb_core::{Fb, browsers};

#[test]
#[ignore = "launches this machine's dedicated BlindFB browser"]
fn dedicated_browser_status() {
    let fb = Fb::discover().expect("install the backend first");
    let status = fb.run(&["status"]).expect("the installed browser must run the backend");
    assert!(status.contains("logged in"), "status must report authentication state");
    println!("Backend browser started; signed in: {}", browsers::signed_in(&status));
    fb.run(&["quit"]).expect("close only the dedicated browser");
}

#[test]
#[ignore = "requires a Facebook session in Brave Stable; close its windows before running"]
fn brave_import_and_facebook_read() {
    let fb = Fb::discover().expect("install the backend first");
    fb.run(&["status"]).expect("start the dedicated engine");
    let browser = browsers::detect_browsers().into_iter()
        .find(|b| b.kind == browsers::BrowserKind::Brave && b.label.is_empty()).expect("install Brave Stable first");
    let status = browsers::import_chromium(&fb, &browser).expect("import Brave session");
    assert!(browsers::signed_in(&status));
    let profile = fb.run(&["profile", "me"]).expect("Facebook must accept the imported session");
    assert!(!profile.trim().is_empty());
    println!("Brave import and authenticated Facebook profile read passed.");
    fb.run(&["quit"]).expect("close the engine to verify the imported session persists");
    assert!(browsers::signed_in(&fb.run(&["status"]).unwrap()));
    fb.run(&["profile", "me"]).expect("Facebook must accept the session after a full browser restart");
    fb.run(&["quit"]).expect("close only the dedicated browser");
}

#[test]
#[ignore = "opens a dedicated Facebook sign-in window for the user"]
fn visible_sign_in_and_facebook_read() {
    let fb = Fb::discover().expect("install the backend first");
    let status = fb.run(&["login", "--visible"]).expect("finish sign-in in the dedicated browser");
    assert!(browsers::signed_in(&status));
    let profile = fb.run(&["profile", "me"]).expect("Facebook must accept the saved session");
    assert!(!profile.trim().is_empty());
    println!("Direct sign-in and authenticated Facebook profile read passed.");
    fb.run(&["quit"]).expect("close only the dedicated browser");
}
