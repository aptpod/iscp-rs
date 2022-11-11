macro_rules! log_err {
    ($level:ident, $e:expr) => {
        if let Err(e) = $e {
            $level!("{}", e);
        }
    };
    ($level:ident, $e:expr, $msg:expr) => {
        if let Err(e) = $e {
            $level!("{}: {:?}", $msg, e);
        }
    };
}

macro_rules! debug_log_err {
    ($level:ident, $e:expr) => {
        if let Err(e) = $e {
            $level!("error: {:?}", e);
        }
    };
}
