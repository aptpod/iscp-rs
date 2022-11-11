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
            r#type: id.r#type,
        }
    }
}

impl DataFilter {
    pub fn new<T: ToString>(name: T, f_type: T) -> Self {
        Self {
            name: name.to_string(),
            r#type: f_type.to_string(),
        }
    }

    fn new_no_filter() -> Self {
        Self::new("#", "#")
    }
}
