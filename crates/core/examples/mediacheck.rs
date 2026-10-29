//! Read-only reader and stream check; never submits a comment, share or reaction.
fn main() -> Result<(), String> {
    let url = std::env::args().nth(1).ok_or("provide a Facebook video or reel URL")?;
    let fb = blindfb_core::Fb::discover().ok_or("install the Facebook backend first")?;
    let page = format!("blindfb-mediacheck-{}", std::process::id());
    let start = std::time::Instant::now();
    let json = blindfb_core::posts::load(&fb, &page, &url)?;
    let result = (|| {
        let detail = blindfb_core::posts::parse_detail(&json)?;
        if let Some(expected) = std::env::args().nth(2) {
            assert!(detail.text.contains(&expected), "reader must expose the expected video title; got {:?}", detail.text);
        }
        assert!(detail.video.is_some(), "reader must expose the video");
        println!(
            "Reader ready in {:?}: {} comments, {} links",
            start.elapsed(),
            detail.comments.len(),
            detail.links.len()
        );
        let start = std::time::Instant::now();
        let stream = blindfb_core::media::stream_url(&fb, &page, &url)?;
        assert!(stream.starts_with("https://"));
        println!("Stream ready in {:?}", start.elapsed());
        Ok(())
    })();
    let _ = blindfb_core::posts::close(&fb, &page);
    result
}
