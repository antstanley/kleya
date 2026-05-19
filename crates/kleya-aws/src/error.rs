use kleya_core::error::{BoxError, Error as CoreError};

#[derive(Debug, thiserror::Error)]
pub enum AwsError {
    #[error("ec2 sdk: {0}")]
    Sdk(#[from] BoxError),
    /// An expected field is missing on a successful SDK response.
    #[error("missing field in response: {0}")]
    MissingField(&'static str),
    /// SSM parameter lookup returned no value.
    #[error("ssm parameter not found: {0}")]
    SsmMissing(String),
    /// A side-effecting call (delete, ensure) appeared to succeed but the
    /// post-condition does not hold. Distinct from `MissingField` (which is
    /// about response shape) — this is "we did the thing and AWS still
    /// disagrees with the world we expected."
    #[error("post-condition violated in {op}: {detail}")]
    PostconditionViolated {
        op: &'static str,
        detail: &'static str,
    },
}

impl From<AwsError> for CoreError {
    fn from(e: AwsError) -> Self {
        CoreError::adapter("aws-ec2", e)
    }
}
