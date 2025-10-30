use mongodb::{Client, Collection};
use once_cell::sync::Lazy;
use bson::Document;
use std::sync::Arc;

pub async fn get_db() -> Arc<Client> {
    let client = Arc::new(
        Client::with_uri_str("mongodb://localhost:27017")
            .await
            .expect("Failed to connect to MongoDB"),
    );
    client
}

pub static CLIENT: Lazy<Arc<Client>> = Lazy::new(|| {
    let rt = tokio::runtime::Handle::current();
    Arc::new(
        rt.block_on(Client::with_uri_str("mongodb://localhost:27017"))
            .expect("Failed to connect to MongoDB"),
    )
});

pub const DB_NAME: &str = "rust_meeting";

pub fn user_collection(client: &Arc<Client>) -> Collection<Document> {
    client.database(DB_NAME).collection("users")
}

pub fn lecture_collection(client: &Arc<Client>) -> Collection<Document> {
    client.database(DB_NAME).collection("lecture")
}

pub fn invitation_collection(client: &Arc<Client>) -> Collection<Document> {
    client.database(DB_NAME).collection("invitation")
}

pub fn feedback_collection(client: &Arc<Client>) -> Collection<Document> {
    client.database(DB_NAME).collection("feedback")
}

pub fn la_collection(client: &Arc<Client>) -> Collection<Document> {
    client.database(DB_NAME).collection("la")
}

pub fn discussion_collection(client: &Arc<Client>) -> Collection<Document> {
    client.database(DB_NAME).collection("discussion")
}