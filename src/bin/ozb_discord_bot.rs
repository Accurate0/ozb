use anyhow::Context;
use futures::FutureExt;
use sqlx::postgres::PgPoolOptions;
use sqlx::{Pool, Postgres};
use std::{error::Error, sync::Arc};
use tracing::{instrument, Level};
use tracing_subscriber::filter::Targets;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use twilight_cache_inmemory::{InMemoryCache, ResourceType};
use twilight_gateway::{Event, EventType, Intents, Shard, ShardId};
use twilight_http::Client as DiscordHttpClient;
use twilight_model::channel::message::{
    component::{SelectMenu, SelectMenuOption},
    MessageFlags, ReactionType,
};
use twilight_standby::Standby;
use twilight_util::builder::InteractionResponseDataBuilder;
use vesper::{
    framework::DefaultError,
    prelude::*,
    twilight_exports::{
        ActionRow, CommandOptionChoice, CommandOptionChoiceValue, Component, Interaction,
        InteractionData, InteractionResponse, InteractionResponseData, InteractionResponseType,
        InteractionType,
    },
};

struct BotContext {
    pool: Pool<Postgres>,
    standby: Standby,
}

#[error_handler]
async fn handle_interaction_error(_ctx: &SlashContext<Arc<BotContext>>, error: DefaultError) {
    tracing::error!("error handling interaction: {:?}", error);
}

#[instrument(skip(ctx))]
#[command("register")]
#[description = "register a keyword...."]
#[error_handler(handle_interaction_error)]
async fn handle_register_keywords(
    ctx: &SlashContext<Arc<BotContext>>,
    #[description = "what u want"] keyword: String,
) -> DefaultCommandResult {
    let response = InteractionResponseDataBuilder::default().flags(MessageFlags::EPHEMERAL);
    ctx.interaction_client
        .create_response(
            ctx.interaction.id,
            &ctx.interaction.token,
            &InteractionResponse {
                kind: InteractionResponseType::DeferredChannelMessageWithSource,
                data: Some(response.build()),
            },
        )
        .await?;

    let interaction = &ctx.interaction;
    let discord_id = interaction
        .author_id()
        .context("must have author")?
        .to_string();

    let channel_id = interaction
        .channel
        .as_ref()
        .context("must be from a channel")?
        .id
        .to_string();

    let queried_categories = sqlx::query!("SELECT id, name, emoji FROM categories")
        .fetch_all(&ctx.data.pool)
        .await?;

    let option_count = queried_categories.len();
    let uuid = uuid::Uuid::new_v4().as_hyphenated().to_string();
    let modal = SelectMenu {
        custom_id: uuid.clone(),
        disabled: false,
        max_values: Some(option_count as u8),
        min_values: Some(1),
        options: queried_categories
            .iter()
            .map(|c| SelectMenuOption {
                default: false,
                description: None,
                emoji: Some(ReactionType::Unicode {
                    name: c.emoji.to_owned(),
                }),
                label: c.name.to_owned(),
                value: c.id.to_string(),
            })
            .collect(),
        placeholder: Some("Select categories".to_owned()),
    };

    let action_row = ActionRow {
        components: [modal.into()].into(),
    };

    let component_message = ctx
        .interaction_client
        .update_response(&ctx.interaction.token)
        .components(Some(&[Component::ActionRow(action_row)]))?
        .await?
        .model()
        .await?;

    let wait_for_selection = ctx
        .data
        .standby
        .wait_for_component(component_message.id, move |i: &Interaction| {
            i.data.clone().map_or(false, |data| match data {
                InteractionData::MessageComponent(m) => m.custom_id == uuid,
                _ => false,
            })
        })
        .await?;

    ctx.interaction_client
        .create_response(
            wait_for_selection.id,
            &wait_for_selection.token,
            &InteractionResponse {
                kind: InteractionResponseType::DeferredUpdateMessage,
                data: None,
            },
        )
        .await?;

    let values = match wait_for_selection.data.unwrap() {
        InteractionData::MessageComponent(m) => m.values,
        _ => return Err(anyhow::Error::msg("this should not happen").into()),
    };

    let categories = if values.iter().any(|v| *v == "1") {
        vec![1]
    } else {
        values
            .into_iter()
            .flat_map(|v| -> Result<i32, anyhow::Error> { Ok(v.parse()?) })
            .collect()
    };

    let named_categories = categories
        .iter()
        .filter_map(|id| -> Option<_> {
            queried_categories
                .iter()
                .find(|c| *id == c.id)
                .map(|c| c.name.to_owned())
        })
        .collect::<Vec<_>>();

    let mut transaction = ctx.data.pool.begin().await?;
    sqlx::query!(
        "INSERT INTO discord_users(discord_id) VALUES ($1) ON CONFLICT DO NOTHING",
        discord_id
    )
    .execute(&mut *transaction)
    .await?;

    let discord_user_id = sqlx::query!(
        "SELECT id FROM discord_users WHERE discord_id = $1",
        discord_id
    )
    .fetch_one(&mut *transaction)
    .await?;

    let discord_notification_id = sqlx::query!(
        "INSERT INTO discord_notifications (channel_id) VALUES ($1) returning id",
        channel_id
    )
    .fetch_one(&mut *transaction)
    .await?;

    sqlx::query!(
        "INSERT INTO registered_keywords (keyword, discord_user_id, discord_notification_id, categories) VALUES ($1, $2, $3, $4)",
        keyword,
        discord_user_id.id,
        discord_notification_id.id,
        &named_categories
    )
    .execute(&mut *transaction)
    .await?;

    transaction.commit().await?;

    ctx.interaction_client
        .update_response(&ctx.interaction.token)
        .content(Some(&format!(
            "Registered \"{}\" as keyword for search with categories: {}",
            keyword,
            named_categories.join(", ")
        )))?
        .components(None)?
        .await?;

    Ok(())
}

#[autocomplete]
async fn autocomplete_existing_keywords(
    ctx: AutocompleteContext<Arc<BotContext>>,
) -> Option<InteractionResponseData> {
    let discord_id = ctx.interaction.author_id()?.to_string();

    let choices = sqlx::query!(
        r#"
        SELECT r.* FROM registered_keywords AS r JOIN discord_users AS du ON r.discord_user_id = du.id WHERE du.discord_id = $1
    "#,
        discord_id
    )
    .fetch_all(&ctx.data.pool)
    .await
    .ok()?
    .iter()
    .map(|item| CommandOptionChoice {
        name: item.keyword.clone(),
        name_localizations: None,
        value: CommandOptionChoiceValue::String(item.id.to_string()),
    })
    .collect();

    Some(InteractionResponseData {
        choices: Some(choices),
        ..Default::default()
    })
}

#[instrument(skip(ctx))]
#[command("unregister")]
#[description = "remove previous registrations..."]
#[error_handler(handle_interaction_error)]
async fn handle_unregister_keywords(
    ctx: &SlashContext<Arc<BotContext>>,
    #[autocomplete(autocomplete_existing_keywords)]
    #[description = "what u want"]
    selection: String,
) -> DefaultCommandResult {
    let response = InteractionResponseDataBuilder::default().flags(MessageFlags::EPHEMERAL);
    ctx.interaction_client
        .create_response(
            ctx.interaction.id,
            &ctx.interaction.token,
            &InteractionResponse {
                kind: InteractionResponseType::DeferredChannelMessageWithSource,
                data: Some(response.build()),
            },
        )
        .await?;

    // TODO: fix this, it can be any option id, regardless of who placed it
    // but that requires knowing the db key (good for me?)
    let deleted_item = sqlx::query!(
        "DELETE FROM registered_keywords WHERE id = $1 RETURNING keyword",
        selection.parse::<i32>()?
    )
    .fetch_one(&ctx.data.pool)
    .await?;

    ctx.interaction_client
        .update_response(&ctx.interaction.token)
        .content(Some(&format!(
            "Removed \"{}\" as keyword for search",
            deleted_item.keyword
        )))?
        .await?;

    Ok(())
}

pub async fn run_discord_bot() -> Result<(), anyhow::Error> {
    let database_url = std::env::var("DATABASE_URL")?;
    let discord_token = std::env::var("DISCORD_TOKEN")?;

    let discord_http = Arc::new(DiscordHttpClient::new(discord_token.to_owned()));
    let standby = Standby::new();
    let mut shard = Shard::new(
        ShardId::ONE,
        discord_token.to_string(),
        Intents::GUILD_MESSAGES | Intents::MESSAGE_CONTENT | Intents::GUILDS,
    );

    tracing::info!("connecting to db: {database_url}");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    let bot_context = Arc::new(BotContext { pool, standby });

    let cache = InMemoryCache::builder()
        .resource_types(ResourceType::MESSAGE | ResourceType::GUILD)
        .build();

    let app_id = discord_http
        .current_user_application()
        .await?
        .model()
        .await?
        .id;

    let framework = Arc::new(
        Framework::builder(discord_http.clone(), app_id, bot_context.clone())
            .command(handle_register_keywords)
            .command(handle_unregister_keywords)
            .build(),
    );

    if let Err(e) = framework.register_global_commands().await {
        tracing::error!("error registering commands: {}", e);
    };

    while let Ok(event) = shard.next_event().await {
        cache.update(&event);
        if matches!(event.kind(), EventType::GatewayHeartbeatAck) {
            continue;
        }

        match event.guild_id() {
            Some(guild_id) => {
                let guild_name = match cache.guild(guild_id) {
                    Some(g) => g.name().to_owned(),
                    None => discord_http.guild(guild_id).await?.model().await?.name,
                };

                tracing::info!("event {:?} from server {:?}", event.kind(), guild_name);
            }
            None => {
                tracing::info!("event {:?}", event.kind());
            }
        }

        if matches!(event.kind(), EventType::Ready) {
            tracing::info!("connected on shard");
            continue;
        }

        if matches!(
            event.kind(),
            EventType::MessageCreate | EventType::MessageUpdate
        ) {
            continue;
        }

        bot_context.standby.process(&event);
        tokio::spawn(
            handle_event(event, Arc::clone(&framework)).then(|result| async {
                match result {
                    Ok(_) => {}
                    Err(e) => tracing::error!("{}", e),
                }
            }),
        );
    }

    Ok(())
}

#[instrument(skip_all)]
async fn handle_event(
    event: Event,
    framework: Arc<Framework<Arc<BotContext>>>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if let Event::InteractionCreate(i) = event {
        match i.kind {
            InteractionType::ApplicationCommand
            | InteractionType::ApplicationCommandAutocomplete => {
                let inner = i.0;
                framework.process(inner).await;
            }
            kind => tracing::info!("ignoring interaction type: {:?}", kind),
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(Targets::default().with_default(Level::INFO))
        .with(tracing_subscriber::fmt::layer())
        .init();

    run_discord_bot().await?;

    Ok(())
}
