#[macro_export]
macro_rules! skip_option {
    ($res:expr, $item:literal) => {
        match $res {
            Some(val) => val,
            None => {
                tracing::warn!("skipping loop because {} missing", $item);
                continue;
            }
        }
    };
}

#[macro_export]
macro_rules! skip_result {
    ($res:expr, $item:literal) => {
        match $res {
            Ok(val) => val,
            Err(e) => {
                tracing::warn!("skipping loop because {} has error: {}", $item, e);
                continue;
            }
        }
    };
}
