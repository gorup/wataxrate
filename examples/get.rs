#[tokio::main]
async fn main() {
    env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .init();

    // Space Needle's Address
    println!("{:#?}", wataxrate::get("400 Broad St", "Seattle", "98109").await);
}
