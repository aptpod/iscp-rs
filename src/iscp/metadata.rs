//! メタデータに関するモジュールです。

/// メタデータ送信時のオプションです。
///
/// Noneが設定されているオプションはサーバーに送信されません。
#[derive(Default)]
pub struct SendMetadataConfig {
    /// 永続化するかどうか
    pub persist: Option<bool>,
}
