use anyhow::{bail, Result};
use edge_core::{CollectionTask, EdgeConfigPackage};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CollectionSchedule {
    tasks: Vec<ScheduledCollectionTask>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ScheduledCollectionTask {
    task_id: String,
    interval_ms: u64,
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
