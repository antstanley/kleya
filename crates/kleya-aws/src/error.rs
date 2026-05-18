use kleya_core::error::{BoxError, Error as CoreError};

#[derive(Debug, thiserror::Error)]
pub enum AwsError {
    #[error("ec2 sdk: {0}")]
    Sdk(#[from] BoxError),
    #[error("missing field in response: {0}")]
    MissingField(&'static str),
    #[error("ssm parameter not found: {0}")]
    SsmMissing(String),
}

impl From<AwsError> for CoreError {
    fn from(e: AwsError) -> Self {
        CoreError::adapter("aws-ec2", e)
    }
}
