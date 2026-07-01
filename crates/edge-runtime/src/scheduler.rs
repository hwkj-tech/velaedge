use anyhow::{bail, Result};
use edge_core::{CollectionTask, DataConfig, EdgeConfigPackage};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CollectionSchedule {
    tasks: Vec<ScheduledCollectionTask>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataConfigSchedule {
    data_configs: Vec<ScheduledDataConfig>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ScheduledCollectionTask {
    task_id: String,
    interval_ms: u64,
    next_due_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ScheduledDataConfig {
    config_id: String,
    period_ms: u64,
    next_due_ms: u64,
}

impl CollectionSchedule {
    pub fn from_package(package: &EdgeConfigPackage) -> Result<Self> {
        let mut tasks = Vec::new();
        for task in package.collection_tasks.iter().filter(|task| task.enabled) {
            validate_task(task)?;
            tasks.push(ScheduledCollectionTask {
                task_id: task.task_id.clone(),
                interval_ms: task.interval_ms,
                next_due_ms: 0,
            });
        }
        Ok(Self { tasks })
    }

    pub fn due_task_ids(&self, now_ms: u64) -> Vec<&str> {
        self.tasks
            .iter()
            .filter_map(|task| {
                if now_ms >= task.next_due_ms {
                    Some(task.task_id.as_str())
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn mark_ran(&mut self, task_id: &str, now_ms: u64) -> Result<()> {
        let Some(task) = self.tasks.iter_mut().find(|task| task.task_id == task_id) else {
            bail!("collection task not scheduled: {task_id}");
        };
        task.next_due_ms = now_ms.saturating_add(task.interval_ms);
        Ok(())
    }
}

impl DataConfigSchedule {
    pub fn from_package(package: &EdgeConfigPackage) -> Result<Self> {
        let mut data_configs = Vec::new();
        for data_config in package.data_configs.iter().filter(|config| config.enabled) {
            validate_data_config(data_config)?;
            data_configs.push(ScheduledDataConfig {
                config_id: data_config.config_id.clone(),
                period_ms: data_config.collection.period_ms,
                next_due_ms: 0,
            });
        }
        Ok(Self { data_configs })
    }

    pub fn due_config_ids(&self, now_ms: u64) -> Vec<&str> {
        self.data_configs
            .iter()
            .filter_map(|data_config| {
                if now_ms >= data_config.next_due_ms {
                    Some(data_config.config_id.as_str())
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn mark_ran(&mut self, config_id: &str, now_ms: u64) -> Result<()> {
        let Some(data_config) = self
            .data_configs
            .iter_mut()
            .find(|data_config| data_config.config_id == config_id)
        else {
            bail!("data config not scheduled: {config_id}");
        };
        data_config.next_due_ms = now_ms.saturating_add(data_config.period_ms);
        Ok(())
    }
}

fn validate_task(task: &CollectionTask) -> Result<()> {
    if task.task_id.trim().is_empty() {
        bail!("collection task id is required");
    }
    if task.interval_ms == 0 {
        bail!("collection task interval must be greater than zero");
    }
    if task.point_ids.is_empty() {
        bail!("collection task must contain at least one point");
    }
    Ok(())
}

fn validate_data_config(data_config: &DataConfig) -> Result<()> {
    if data_config.config_id.trim().is_empty() {
        bail!("data config id is required");
    }
    if data_config.collection.period_ms == 0 {
        bail!("data config collection period must be greater than zero");
    }
    if data_config.points.is_empty() {
        bail!("data config must contain at least one point");
    }
    Ok(())
}
