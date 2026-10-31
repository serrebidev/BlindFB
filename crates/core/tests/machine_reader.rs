//! Read-only checks for Facebook layouts; require this machine's signed-in browser.
use blindfb_core::{posts, Fb};

#[test]
#[ignore = "reads a live Facebook photo and its parent post"]
fn photo_opens_original_post() {
    let fb = Fb::discover().expect("install the backend first");
    let page = format!("blindfb-photo-check-{}", std::process::id());
    let result = posts::load(
        &fb,
        &page,
        "https://www.facebook.com/photo/?fbid=1793212072814537",
    )
    .and_then(|json| posts::parse_detail(&json));
    let _ = posts::close(&fb, &page);
    let detail = result.expect("photo should open its parent post");
    assert!(
        detail.url.contains("/bigfeastbistro/posts/"),
        "must retain the original post identity"
    );
    assert!(detail.author.contains("Big Feast"));
    assert!(
        !detail.text.trim().is_empty(),
        "parent post text must be readable"
    );
    assert_eq!(detail.links.last().unwrap().url, detail.url);
}
