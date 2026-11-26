//! Batch Drive Explorer operations on `Bridge` with partial-progress accounting.

use crate::runtime::bridge::Bridge;
use opendrive_sync::NodeId;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct BatchOutcome {
    pub succeeded_ids: Vec<NodeId>,
    pub message: String,
    pub error: Option<String>,
}

fn format_batch_err(err: String, done: usize, total: usize) -> String {
    if done > 0 {
        format!("{err} ({done} of {total} completed)")
    } else {
        err
    }
}

fn run_batch_steps(
    node_ids: &[NodeId],
    precheck: Result<(), String>,
    mut step: impl FnMut(NodeId) -> Result<(), String>,
) -> (Vec<NodeId>, Option<String>) {
    if let Err(err) = precheck {
        return (Vec::new(), Some(err));
    }
    let (mut ok_ids, mut err_out) = (Vec::new(), None);
    for &id in node_ids {
        match step(id) {
            Ok(()) => ok_ids.push(id),
            Err(err) => {
                err_out = Some(format_batch_err(err, ok_ids.len(), node_ids.len()));
                break;
            }
        }
    }
    (ok_ids, err_out)
}

impl Bridge {
    fn run_transfer_batch(
        &self,
        account_id: Uuid,
        node_ids: &[NodeId],
        verb: &str,
        mut op: impl FnMut(NodeId) -> Result<String, String>,
    ) -> BatchOutcome {
        let mut last_name = String::new();
        let (ok_ids, error) =
            run_batch_steps(node_ids, self.ensure_writable_account(account_id), |id| {
                last_name = op(id)?;
                Ok(())
            });
        let message = if ok_ids.len() == 1 {
            format!("{verb} '{last_name}'")
        } else {
            format!("{verb} {} items", ok_ids.len())
        };
        BatchOutcome {
            succeeded_ids: ok_ids,
            message,
            error,
        }
    }

    pub fn explorer_sync_down_batch(&self, account_id: Uuid, node_ids: &[NodeId]) -> BatchOutcome {
        let mut downloaded = 0usize;
        let (succeeded_ids, error) = run_batch_steps(node_ids, Ok(()), |id| {
            downloaded += self.explorer_sync_down(account_id, id)?.0.downloaded_files;
            Ok(())
        });
        BatchOutcome {
            succeeded_ids,
            message: format!("Synced down {downloaded} file(s)"),
            error,
        }
    }

    pub fn explorer_sync_up_batch(&self, account_id: Uuid, node_ids: &[NodeId]) -> BatchOutcome {
        let mut uploaded = 0usize;
        let (succeeded_ids, error) =
            run_batch_steps(node_ids, self.ensure_writable_account(account_id), |id| {
                uploaded += self.explorer_sync_up_node(account_id, id)?.uploaded_files;
                Ok(())
            });
        BatchOutcome {
            succeeded_ids,
            message: format!("Synced up {uploaded} file(s)"),
            error,
        }
    }

    pub fn explorer_sync_clear_batch(&self, account_id: Uuid, node_ids: &[NodeId]) -> BatchOutcome {
        let (mut uploaded, mut cleared) = (0usize, 0usize);
        let (succeeded_ids, error) =
            run_batch_steps(node_ids, self.ensure_writable_account(account_id), |id| {
                let sum = self.explorer_sync_up_and_clear(account_id, id)?;
                uploaded += sum.uploaded_files;
                cleared += sum.cleared_files;
                Ok(())
            });
        BatchOutcome {
            succeeded_ids,
            message: format!("Synced {uploaded} and cleared {cleared} local replica(s)"),
            error,
        }
    }

    pub fn explorer_delete_batch(&self, account_id: Uuid, node_ids: &[NodeId]) -> BatchOutcome {
        let (succeeded_ids, error) =
            run_batch_steps(node_ids, self.ensure_writable_account(account_id), |id| {
                self.explorer_delete_item(account_id, id)
            });
        let message = if succeeded_ids.len() == 1 {
            "Moved item to Trash and removed local copy".into()
        } else {
            format!(
                "Moved {} items to Trash and removed local copies",
                succeeded_ids.len()
            )
        };
        BatchOutcome {
            succeeded_ids,
            message,
            error,
        }
    }

    pub fn explorer_copy_batch(
        &self,
        account_id: Uuid,
        node_ids: &[NodeId],
        dest_folder: NodeId,
    ) -> BatchOutcome {
        self.run_transfer_batch(account_id, node_ids, "Copied", |id| {
            self.explorer_copy_item(account_id, id, dest_folder)
                .map(|e| e.name)
        })
    }

    pub fn explorer_move_batch(
        &self,
        account_id: Uuid,
        node_ids: &[NodeId],
        dest_folder: NodeId,
    ) -> BatchOutcome {
        self.run_transfer_batch(account_id, node_ids, "Moved", |id| {
            self.explorer_move_item(account_id, id, dest_folder)
                .map(|e| e.name)
        })
    }
}
