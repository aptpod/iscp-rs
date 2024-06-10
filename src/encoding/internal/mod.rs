#[rustfmt::skip]
#[allow(clippy::all)]
pub mod autogen {
    include!("autogen/iscp2.v1.rs");

    #[path = "iscp2.v1.extensions.rs"]
    pub mod extensions;
}
