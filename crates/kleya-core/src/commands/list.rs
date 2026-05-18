use std::sync::Arc;

use crate::model::instance::{Instance, InstanceFilter};
use crate::ports::cloud_compute::CloudCompute;
use crate::Result;

pub struct ListService {
    pub compute: Arc<dyn CloudCompute>,
}

impl ListService {
    pub async fn list_managed(&self) -> Result<Vec<Instance>> {
        let filter = InstanceFilter {
            name: None,
            managed_only: true,
            states: vec![],
        };
        self.compute.instance_list(&filter).await
    }
}
