use anyhow::Context;
use futures::FutureExt;
use ozb::{
    prisma::{
        self,
        read_filters::StringFilter,
        registered_keywords::{UniqueWhereParam, WhereParam},
    },
    types::{ApplicationConfig, BotContext},
};
use std::{error::Error, sync::Arc};
use tracing::instrument;
use twilight_cache_inmemory::{InMemoryCache, ResourceType};
use twilight_gateway::{Event, EventType, Intents, Shard, ShardId};
use twilight_http::Client as DiscordHttpClient;
use twilight_model::{
    channel::message::MessageFlags,
    gateway::{
        payload::outgoing::UpdatePresence,
        presence::{ActivityType, MinimalActivity, Status},
    },
};
use zephyrus::{
    framework::DefaultError,
    prelude::*,
    twilight_exports::{
        CommandOptionChoice, CommandOptionChoiceValue, InteractionResponseData, InteractionType,
    },
};

#[error_handler]
async fn handle_interaction_error(_ctx: &SlashContext<Arc<BotContext>>, error: DefaultError) {
    log::error!("error handling interaction: {:?}", error);
}

#[instrument(skip(ctx))]
#[command("register")]
#[description = "register a keyword...."]
#[error_handler(handle_interaction_error)]
async fn handle_register_keywords(
    ctx: &SlashContext<Arc<BotContext>>,
    #[description = "what u want"] keyword: String,
) -> DefaultCommandResult {
    ctx.acknowledge().await?;

    let prisma_client = &ctx.data.prisma_client;
    let discord_id = ctx
        .interaction
        .author_id()
        .context("must have author")?
        .to_string();

    let channel_id = ctx
        .interaction
        .channel_id
        .context("must be from a channel")?
        .to_string();

    prisma_client
        .registered_keywords()
        .create(keyword.clone(), discord_id, channel_id, vec![])
        .exec()
        .await?;

    ctx.interaction_client
        .create_followup(&ctx.interaction.token)
        .content(&format!("Registered \"{}\" as keyword for search", keyword))?
        .flags(MessageFlags::EPHEMERAL)
        .await?;

    Ok(())
}

#[autocomplete]
async fn autocomplete_existing_keywords(
    ctx: AutocompleteContext<Arc<BotContext>>,
) -> Option<InteractionResponseData> {
    let discord_id = ctx.interaction.author_id()?.to_string();

    let choices = ctx
        .data
        .prisma_client
        .registered_keywords()
        .find_many(vec![WhereParam::UserId(StringFilter::Equals(discord_id))])
        .exec()
        .await
        .ok()?
        .iter()
        .map(|item| CommandOptionChoice {
            name: item.keyword.clone(),
            name_localizations: None,
            value: CommandOptionChoiceValue::String(item.id.clone()),
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
    // todo: fix this, it can be any option id, regardless of who placed it
    // but that requires knowing the db key (good for me?)
    option_id: String,
) -> DefaultCommandResult {
    ctx.acknowledge().await?;

    let prisma_client = &ctx.data.prisma_client;
    let deleted_item = prisma_client
        .registered_keywords()
        .delete(UniqueWhereParam::IdEquals(option_id))
        .exec()
        .await?;

    ctx.interaction_client
        .create_followup(&ctx.interaction.token)
        .content(&format!(
            "Removed \"{}\" as keyword for search",
            deleted_item.keyword
        ))?
        .await?;

    Ok(())
}

pub async fn run_discord_bot(config: ApplicationConfig) -> Result<(), anyhow::Error> {
    let discord_token = config.discord_token.clone();
    let discord_http = Arc::new(DiscordHttpClient::new(discord_token.to_owned()));

    let mut shard = Shard::new(
        ShardId::ONE,
        discord_token.to_string(),
        Intents::GUILD_MESSAGES | Intents::MESSAGE_CONTENT | Intents::GUILDS,
    );

    let prisma_client = prisma::new_client_with_url(&config.mongodb_connection_string).await?;

    let bot_context = Arc::new(BotContext {
        config,
        prisma_client,
    });

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
        log::error!("error registering commands: {}", e);
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

                log::info!("event {:?} from server {:?}", event.kind(), guild_name);
            }
            None => {
                log::info!("event {:?}", event.kind());
            }
        }

        if matches!(event.kind(), EventType::Ready) {
            log::info!("connected on shard");

            let activity = MinimalActivity {
                kind: ActivityType::Listening,
                name: "THE BADDEST by K/DA".to_owned(),
                url: None,
            }
            .into();

            let request = UpdatePresence::new([activity], false, None, Status::Online)?;
            let result = shard.command(&request).await;
            log::info!("presence update: {:?}", result);

            continue;
        }

        if matches!(
            event.kind(),
            EventType::MessageCreate | EventType::MessageUpdate
        ) {
            continue;
        }

        tokio::spawn(
            handle_event(
                event,
                Arc::clone(&discord_http),
                Arc::clone(&bot_context),
                Arc::clone(&framework),
            )
            .then(|result| async {
                match result {
                    Ok(_) => {}
                    Err(e) => log::error!("{}", e),
                }
            }),
        );
    }

    Ok(())
}

#[instrument(skip_all)]
async fn handle_event(
    event: Event,
    _discord: Arc<DiscordHttpClient>,
    _ctx: Arc<BotContext>,
    framework: Arc<Framework<Arc<BotContext>>>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if let Event::InteractionCreate(i) = event {
        match i.kind {
            InteractionType::ApplicationCommand
            | InteractionType::ApplicationCommandAutocomplete => {
                let inner = i.0;
                framework.process(inner).await;
            }
            kind => log::info!("ignoring interaction type: {:?}", kind),
        }
    }

    Ok(())
}
