use crate::prisma::PrismaClient;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Debug)]
pub struct ApplicationConfig {
    #[cfg(debug_assertions)]
    #[serde(rename = "DiscordAuthToken-dev")]
    pub discord_token: String,

    #[cfg(not(debug_assertions))]
    #[serde(rename = "DiscordAuthToken")]
    pub discord_token: String,

    #[cfg(not(debug_assertions))]
    #[serde(rename = "MongoDbConnectionString")]
    pub mongodb_connection_string: String,

    #[cfg(debug_assertions)]
    #[serde(rename = "MongoDbConnectionString-dev")]
    pub mongodb_connection_string: String,
}

#[derive(Debug)]
pub struct BotContext {
    pub config: ApplicationConfig,
    pub prisma_client: PrismaClient,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MongoDbPayload {
    pub version: String,
    pub id: String,
    #[serde(rename = "detail-type")]
    pub detail_type: String,
    pub source: String,
    pub account: String,
    pub time: String,
    pub region: String,
    pub resources: Vec<String>,
    pub detail: Detail,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Detail {
    #[serde(rename = "_id")]
    pub id: Id,
    pub operation_type: String,
    pub cluster_time: ClusterTime,
    pub full_document: FullDocument,
    pub ns: Ns,
    pub document_key: DocumentKey,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Id {
    #[serde(rename = "_data")]
    pub data: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClusterTime {
    #[serde(rename = "T")]
    pub t: i64,
    #[serde(rename = "I")]
    pub i: i64,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FullDocument {
    #[serde(rename = "_id")]
    pub id: String,
    #[serde(rename = "postId")]
    pub post_id: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ns {
    pub db: String,
    pub coll: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentKey {
    #[serde(rename = "_id")]
    pub id: String,
}
