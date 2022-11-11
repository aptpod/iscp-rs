pub use crate::encoding::internal::autogen::ResultCode;

impl ResultCode {
    pub const NORMAL_CLOSURE: Self = Self::Succeeded;
}

impl From<i32> for ResultCode {
    fn from(result_code: i32) -> Self {
        Self::from_i32(result_code).unwrap()
    }
}

impl ResultCode {
    pub fn is_succeeded(&self) -> bool {
        Self::Succeeded == *self
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn is_succeeded() {
        assert!(ResultCode::Succeeded.is_succeeded());
        assert!(!ResultCode::IncompatibleVersion.is_succeeded());
        assert!(!ResultCode::MaximumDataIdAlias.is_succeeded());
        assert!(!ResultCode::MaximumUpstreamAlias.is_succeeded());
        assert!(!ResultCode::UnspecifiedError.is_succeeded());
    }
}
