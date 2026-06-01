pub mod constitutional;
pub mod data;
pub mod error;
pub mod judicial;
pub mod moj_openapi;
pub mod mojlaw;
pub mod opendata;
pub mod regulations;
pub mod sources;

pub use error::TwlawError;

pub type TwlawResult<T> = Result<T, TwlawError>;

pub fn retrieved_at() -> String {
    chrono::Utc::now().to_rfc3339()
}
