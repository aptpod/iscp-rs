pub use crate::encoding::internal::autogen::DownstreamFilter;

impl DownstreamFilter {
    pub fn new_no_data_filters<T: ToString>(source_node_id: T) -> Self {
        Self {
            source_node_id: source_node_id.to_string(),
            data_filters: vec![DataFilter::new_no_filter()],
        }
    }
}

pub use crate::encoding::internal::autogen::DataFilter;

impl From<super::DataId> for DataFilter {
    fn from(id: super::DataId) -> Self {
        Self {
            name: id.name,
            type_: id.type_,
        }
    }
}

impl DataFilter {
    pub fn new<T1: ToString, T2: ToString>(name: T1, type_: T2) -> Self {
        Self {
            name: name.to_string(),
            type_: type_.to_string(),
        }
    }

    fn new_no_filter() -> Self {
        Self::new("#", "#")
    }
}
