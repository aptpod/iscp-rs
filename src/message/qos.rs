pub use crate::encoding::internal::autogen::QoS;

impl From<i32> for QoS {
    fn from(qos: i32) -> Self {
        Self::from_i32(qos).unwrap()
    }
}
