pub mod cfg {
    pub const OZB_RSS_DEALS_URL: &str = "https://www.ozbargain.com.au/deals/feed";
    pub const UPTIME_DAEMON_PUSH_URL: &str =
        "https://uptime.anurag.sh/checkin/Qo2HHYuzUIzH0SfUCdRnoWjQ2qBhbhye";
    pub const UPTIME_BOT_PUSH_URL: &str =
        "https://uptime.anurag.sh/checkin/hackBHlWLM4MSG3wocmnSg3EJGpNIW0b";
    #[cfg(debug_assertions)]
    pub const REDIS_KEY_PREFIX: &str = "OZB_DEBUG";
    #[cfg(not(debug_assertions))]
    pub const REDIS_KEY_PREFIX: &str = "OZB";
}
