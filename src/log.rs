use anyhow::Context;

pub fn init_logger() {
    let cfg = fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "[{}][{}] {}",
                record.level(),
                record.target(),
                message
            ))
        })
        .level(log::LevelFilter::Info);

    let cfg = cfg
        .level_for(
            "aws_smithy_http_tower::parse_response",
            log::LevelFilter::Warn,
        )
        .level_for(
            "aws_config::default_provider::credentials",
            log::LevelFilter::Warn,
        );

    cfg.chain(std::io::stdout())
        .apply()
        .context("failed to set up logger")
        .unwrap();
}
