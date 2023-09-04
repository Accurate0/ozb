pub mod cfg {
    pub const OZB_RSS_DEALS_URL: &str = "https://www.ozbargain.com.au/deals/feed";
    pub const UPTIME_PUSH_URL: &str =
        "https://uptime.anurag.sh/api/push/UbclEdv2wQ?status=up&msg=OK&ping=";
    #[cfg(debug_assertions)]
    pub const REDIS_KEY_PREFIX: &str = "OZB_DEBUG";
    #[cfg(not(debug_assertions))]
    pub const REDIS_KEY_PREFIX: &str = "OZB";
}
