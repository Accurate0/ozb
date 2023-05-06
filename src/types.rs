use crate::prisma::PrismaClient;
use serde::{Deserialize, Serialize};
use strum_macros::{Display, EnumCount, EnumIter, EnumProperty};
use twilight_standby::Standby;

#[derive(Deserialize, Debug)]
pub struct ApplicationConfig {
    #[serde(alias = "discordauthtoken")]
    pub discord_token: String,

    #[serde(alias = "mongodbconnectionstring")]
    pub mongodb_connection_string: String,

    #[serde(alias = "redisconnectionstring")]
    pub redis_connection_string: String,
}

#[derive(Debug)]
pub struct BotContext {
    pub config: ApplicationConfig,
    pub prisma_client: PrismaClient,
    pub standby: Standby,
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

#[derive(EnumCount, Display, EnumIter, EnumProperty)]
pub enum Categories {
    #[strum(props(emoji = "🌏"))]
    All,

    #[strum(props(emoji = "🍺"))]
    Alcohol,

    #[strum(props(emoji = "🏎️"))]
    Automotive,

    #[strum(serialize = "Books & Magazines", props(emoji = "📚"))]
    BooksAndMagazines,

    #[strum(props(emoji = "🖥️"))]
    Computing,

    #[strum(serialize = "Dining & Takeaway", props(emoji = "🥡"))]
    DiningAndTakeaway,

    #[strum(props(emoji = "🧮"))]
    Education,

    #[strum(serialize = "Electrical & Electronics", props(emoji = "🔌"))]
    ElectricalAndElectronics,

    #[strum(props(emoji = "💃"))]
    Entertainment,

    #[strum(serialize = "Fashion & Apparel", props(emoji = "👜"))]
    FashionAndApparel,

    #[strum(props(emoji = "💸"))]
    Financial,

    #[strum(props(emoji = "🎮"))]
    Gaming,

    #[strum(props(emoji = "🛍️"))]
    Groceries,

    #[strum(serialize = "Health & Beauty", props(emoji = "🏥"))]
    HealthAndBeauty,

    #[strum(serialize = "Home & Garden", props(emoji = "🏡"))]
    HomeAndGarden,

    #[strum(props(emoji = "🌐"))]
    Internet,

    #[strum(props(emoji = "📱"))]
    Mobile,

    #[strum(props(emoji = "🐈"))]
    Pets,

    #[strum(serialize = "Sports & Outdoors", props(emoji = "🏏"))]
    SportsAndOutdoors,

    #[strum(serialize = "Toys & Kids", props(emoji = "🪅"))]
    ToysAndKids,

    #[strum(props(emoji = "🛫"))]
    Travel,

    #[strum(props(emoji = "🎲"))]
    Other,
}
