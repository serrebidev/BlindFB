//! Read-only discovery check. Optional argument: a grouped reel notification's full target URL.
fn main() -> Result<(), String> {
    let source =
        std::env::args()
            .nth(1)
            .map_or(blindfb_core::reels::ReelSource::Recommended, |target| {
                let urls = std::env::args().skip(2).collect();
                blindfb_core::reels::ReelSource::Notification {
                    target,
                    author: "Notification account".to_owned(),
                    urls,
                    account: None,
                }
            });
    let fb = blindfb_core::Fb::discover().ok_or("backend missing")?;
    let output = blindfb_core::reels::load(&fb, &source, false)?;
    let (rows, notice) = blindfb_core::reels::parse(&output)?;
    assert!(!rows.is_empty(), "expected at least one reel");
    println!("{} reels; notice: {:?}", rows.len(), notice);
    for row in &rows {
        println!("{}: {} — {}", row.field(0), row.field(1), row.field(4));
    }
    if matches!(source, blindfb_core::reels::ReelSource::Recommended) {
        let (more, _) =
            blindfb_core::reels::parse(&blindfb_core::reels::load(&fb, &source, true)?)?;
        assert!(
            more.iter()
                .all(|row| !rows.iter().any(|old| old.field(0) == row.field(0))),
            "load more must not repeat reels"
        );
        println!("{} additional reels", more.len());
    }
    blindfb_core::reels::close_all(&fb);
    Ok(())
}
