pub mod ozbargain;
pub mod util;

#[derive(Debug, sqlx::Type)]
pub struct RegisteredKeywords {
    pub id: i32,
    pub keyword: String,
    pub owner: i32,
    pub notification_id: i32,
    pub categories: Vec<i32>,
}

#[derive(Debug, sqlx::Type)]
pub struct AutocompleteKeywords {
    pub id: i32,
    pub keyword: String,
    pub categories: Vec<String>,
}
