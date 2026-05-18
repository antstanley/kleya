use std::sync::Arc;

use crate::config::Config;
use crate::model::template::{
    TemplateId, TemplateName, TemplateSpec, TemplateSummary, TemplateVersion,
};
use crate::ports::cloud_compute::CloudCompute;
use crate::Result;

pub struct TemplateService {
    pub compute: Arc<dyn CloudCompute>,
    pub config: Arc<Config>,
}

impl TemplateService {
    pub async fn create(&self, spec: TemplateSpec) -> Result<TemplateId> {
        assert!(!spec.name.0.is_empty(), "template name empty");
        self.compute.template_create(&spec).await
    }

    pub async fn update(&self, id: &TemplateId, spec: TemplateSpec) -> Result<TemplateVersion> {
        assert!(!id.0.is_empty(), "template id empty");
        self.compute.template_update(id, &spec).await
    }

    pub async fn list(&self) -> Result<Vec<TemplateSummary>> {
        self.compute.template_list().await
    }

    pub async fn delete_by_name(&self, name: &TemplateName) -> Result<()> {
        let summary = self
            .compute
            .template_get_by_name(name)
            .await?
            .ok_or_else(|| crate::error::Error::ConfigInvalid {
                reason: format!("template '{}' not found", name.0),
            })?;
        self.compute.template_delete(&summary.id).await
    }
}
