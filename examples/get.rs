#[tokio::main]
async fn main() {
    // Space Needle's Address
    println!("{:#?}", wataxrate::get("400 Broad St", "Seattle", "98109").await);
}
