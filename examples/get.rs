#[tokio::main]
async fn main() {
    println!("{:?}", wataxrate::get("", "", "").await);
}
