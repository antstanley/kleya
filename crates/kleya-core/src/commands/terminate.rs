use std::sync::Arc;

use crate::error::Error;
use crate::model::instance::{InstanceFilter, InstanceId};
use crate::ports::cloud_compute::CloudCompute;
use crate::Result;

pub struct TerminateService {
    pub compute: Arc<dyn CloudCompute>,
    pub region: String,
}

impl TerminateService {
    pub async fn terminate_by_handle(&self, handle: &str) -> Result<InstanceId> {
        let id = if handle.starts_with("i-") {
            InstanceId::new(handle)?
        } else {
            let candidates = self
                .compute
                .instance_list(&InstanceFilter {
                    name: Some(handle.into()),
                    managed_only: true,
                    states: vec![],
                })
                .await?;
            match candidates.len() {
                0 => {
                    return Err(Error::InstanceNotFound {
                        name: handle.into(),
                        region: self.region.clone(),
                    });
                }
                1 => candidates[0].id.clone(),
                _ => {
                    return Err(Error::AmbiguousHandle {
                        name: handle.into(),
                        candidates: candidates.into_iter().map(|i| i.id).collect(),
                    });
                }
            }
        };
        self.compute.instance_terminate(&id).await?;
        Ok(id)
    }
}
