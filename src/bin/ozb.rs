use anyhow::Context;
use futures::FutureExt;
use ozb::config::get_application_config;
use ozb::constants::cfg::UPTIME_BOT_PUSH_URL;
use ozb::http::get_default_http_client;
use ozb::log::init_logger;
use ozb::{
    prisma::{
        self,
        read_filters::StringFilter,
        registered_keywords::{self, UniqueWhereParam, WhereParam},
    },
    types::{ApplicationConfig, BotContext, Categories},
};
use std::time::Duration;
use std::{error::Error, sync::Arc};
use strum::EnumProperty;
use strum::IntoEnumIterator;
use tracing::instrument;
use twilight_cache_inmemory::{InMemoryCache, ResourceType};
use twilight_gateway::{Event, EventType, Intents, Shard, ShardId};
use twilight_http::Client as DiscordHttpClient;
use twilight_model::channel::message::{
    component::{SelectMenu, SelectMenuOption},
    MessageFlags, ReactionType,
};
use twilight_standby::Standby;
use twilight_util::builder::InteractionResponseDataBuilder;
use zephyrus::{
    framework::DefaultError,
    prelude::*,
    twilight_exports::{
        ActionRow, CommandOptionChoice, CommandOptionChoiceValue, Component, Interaction,
        InteractionData, InteractionResponse, InteractionResponseData, InteractionResponseType,
        InteractionType,
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

    let prisma_client = &ctx.data.prisma_client;
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

    let categories = Categories::iter();
    let option_count = categories.len();
    let uuid = uuid::Uuid::new_v4().as_hyphenated().to_string();
    let modal = SelectMenu {
        custom_id: uuid.clone(),
        disabled: false,
        max_values: Some(option_count as u8),
        min_values: Some(1),
        options: categories
            .map(|c| SelectMenuOption {
                default: false,
                description: None,
                emoji: Some(ReactionType::Unicode {
                    name: c.get_str("emoji").unwrap_or_default().to_string(),
                }),
                label: c.to_string(),
                value: c.to_string(),
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

    let categories = if values.iter().any(|v| *v == Categories::All.to_string()) {
        vec![Categories::All.to_string()]
    } else {
        values
    };

    prisma_client
        .registered_keywords()
        .create(
            keyword.clone(),
            discord_id,
            channel_id,
            vec![registered_keywords::categories::set(categories.clone())],
        )
        .exec()
        .await?;

    ctx.interaction_client
        .update_response(&ctx.interaction.token)
        .content(Some(&format!(
            "Registered \"{}\" as keyword for search with categories: {}",
            keyword,
            categories.join(", ")
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
            name: format!(
                "{} ({})",
                item.keyword.clone(),
                if item.categories.is_empty() {
                    Categories::All.to_string()
                } else {
                    item.categories.join(", ")
                },
            ),
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

    let prisma_client = &ctx.data.prisma_client;
    let deleted_item = prisma_client
        .registered_keywords()
        .delete(UniqueWhereParam::IdEquals(option_id))
        .exec()
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

pub async fn run_discord_bot(config: ApplicationConfig) -> Result<(), anyhow::Error> {
    let discord_token = config.discord_token.clone();
    let discord_http = Arc::new(DiscordHttpClient::new(discord_token.to_owned()));
    let standby = Standby::new();
    let mut shard = Shard::new(
        ShardId::ONE,
        discord_token.to_string(),
        Intents::GUILD_MESSAGES | Intents::MESSAGE_CONTENT | Intents::GUILDS,
    );

    let prisma_client = prisma::new_client_with_url(&config.mongodb_connection_string).await?;

    let bot_context = Arc::new(BotContext {
        config,
        prisma_client,
        standby,
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

    let http_client = get_default_http_client();
    tokio::spawn(async move {
        loop {
            let _ = http_client.get(UPTIME_BOT_PUSH_URL).send().await;
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    });

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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_logger();

    let config = get_application_config().await?;
    run_discord_bot(config).await?;

    Ok(())
}
