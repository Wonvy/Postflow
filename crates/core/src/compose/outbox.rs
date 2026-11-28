//! 定时发送：信和附件先存在本机目录的 `outbox/<编号>/` 下，到时间由调度器（每分钟一次）照常发出。
//! 应用关着的时候发不了，下次打开时补发。取消时存回邮箱的「草稿」里，可以接着改。
//!
//! 只在这台设备上：信里可能有附件，不放进同步目录。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use shibapost_mail::Attachment;

use super::{Letter, recipients};
use crate::refresh::now;
use crate::rpc::{parse, reply};
use crate::{Core, RpcError, Scope, Session};

const DIR: &str = "outbox";
const JOB: &str = "job.json";

#[derive(Serialize, Deserialize)]
struct Job {
    id: String,
    /// 什么时候发（毫秒）。
    send_at: i64,
    /// 信本身；附件不在这里，在同一个目录的 0、1、2… 文件里。
    letter: Letter,
    files: Vec<FileMeta>,
    /// 上次发送失败的原因。失败的不再自动重试，等用户「立即发送」或取消。
    #[serde(default)]
    error: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct FileMeta {
    name: String,
    mime: String,
}

#[derive(Deserialize)]
struct Schedule {
    #[serde(flatten)]
    letter: Letter,
    send_at: i64,
}

#[derive(Deserialize)]
struct JobId {
    id: String,
}

/// 界面上「定时发送」列表里的一封。
#[derive(Serialize)]
struct Summary {
    id: String,
    send_at: i64,
    account_id: String,
    to: Vec<String>,
    subject: String,
    attachments: usize,
    error: Option<String>,
}

fn dir(core: &Core) -> PathBuf {
    core.local_dir.join(DIR)
}

fn load(path: &Path) -> Option<Job> {
    serde_json::from_slice(&std::fs::read(path.join(JOB)).ok()?).ok()
}

fn save(path: &Path, job: &Job) -> Result<(), RpcError> {
    let bytes = serde_json::to_vec(job).map_err(|e| RpcError::new("internal", e.to_string()))?;
    std::fs::write(path.join(JOB), bytes).map_err(|e| RpcError::new("io", e.to_string()))
}

fn jobs(core: &Core) -> Vec<(PathBuf, Job)> {
    let Ok(entries) = std::fs::read_dir(dir(core)) else {
        return Vec::new();
    };
    let mut out: Vec<(PathBuf, Job)> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter_map(|p| load(&p).map(|j| (p, j)))
        .collect();
    out.sort_by_key(|(_, j)| j.send_at);
    out
}

/// 编号只要在这个目录里不重复就行。
fn new_id() -> String {
    format!("{:x}{:04x}", now(), rand_suffix())
}

fn rand_suffix() -> u16 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    (nanos ^ (nanos >> 16)) as u16
}

impl Core {
    pub(super) async fn outbox_rpc(
        self: &Arc<Self>,
        method: &str,
        params: Value,
        session: &Arc<Session>,
    ) -> Option<Result<Value, RpcError>> {
        let result = match method {
            "mail.schedule" => self.schedule(params),
            "mail.outbox" => reply(json!({ "items": self.outbox_list() })),
            "mail.outbox_send" => match parse::<JobId>(params) {
                Ok(p) => self
                    .send_job(session, &p.id)
                    .await
                    .and_then(|()| reply(json!({}))),
                Err(e) => Err(e),
            },
            "mail.unschedule" => match parse::<JobId>(params) {
                Ok(p) => self.unschedule(session, &p.id).await,
                Err(e) => Err(e),
            },
            _ => return None,
        };
        Some(result)
    }

    fn schedule(&self, params: Value) -> Result<Value, RpcError> {
        let Schedule {
            mut letter,
            send_at,
        } = parse(params)?;
        // 和立刻发送一样先检查地址，免得到时间才发现发不出去
        let all = [
            recipients(&letter.to)?,
            recipients(&letter.cc)?,
            recipients(&letter.bcc)?,
        ];
        if all.iter().all(Vec::is_empty) {
            return Err(RpcError::new("send_bad_address", ""));
        }
        if send_at <= now() {
            return Err(RpcError::new("invalid", "pick a time in the future"));
        }
        let files = self.take_attachments(&letter.attachments)?;
        let id = new_id();
        let path = dir(self).join(&id);
        std::fs::create_dir_all(&path).map_err(|e| RpcError::new("io", e.to_string()))?;
        for (i, f) in files.iter().enumerate() {
            std::fs::write(path.join(i.to_string()), &f.data)
                .map_err(|e| RpcError::new("io", e.to_string()))?;
        }
        {
            let mut uploads = self.uploads();
            for t in &letter.attachments {
                uploads.files.remove(t);
            }
        }
        letter.attachments.clear();
        let job = Job {
            id: id.clone(),
            send_at,
            letter,
            files: files
                .iter()
                .map(|f| FileMeta {
                    name: f.name.clone(),
                    mime: f.mime.clone(),
                })
                .collect(),
            error: None,
        };
        save(&path, &job)?;
        tracing::info!(attachments = job.files.len(), "mail scheduled");
        self.emit_changed(&[Scope::Items], None);
        reply(json!({ "id": id }))
    }

    fn outbox_list(&self) -> Vec<Summary> {
        jobs(self)
            .into_iter()
            .map(|(_, j)| Summary {
                id: j.id,
                send_at: j.send_at,
                account_id: j.letter.account_id.clone(),
                to: j.letter.to.clone(),
                subject: j.letter.subject.clone(),
                attachments: j.files.len(),
                error: j.error,
            })
            .collect()
    }

    /// 把存着的附件交回给写信用的临时存放处，信里换成新的编号。
    fn restore(&self, path: &Path, job: &Job) -> Result<Letter, RpcError> {
        let mut letter = job.letter.clone();
        let mut uploads = self.uploads();
        for (i, meta) in job.files.iter().enumerate() {
            let data = std::fs::read(path.join(i.to_string()))
                .map_err(|e| RpcError::new("io", e.to_string()))?;
            letter.attachments.push(uploads.insert(Attachment {
                name: meta.name.clone(),
                mime: meta.mime.clone(),
                data,
            }));
        }
        Ok(letter)
    }

    async fn send_job(self: &Arc<Self>, session: &Arc<Session>, id: &str) -> Result<(), RpcError> {
        let path = dir(self).join(id);
        let mut job =
            load(&path).ok_or_else(|| RpcError::new("not_found", "already sent or cancelled"))?;
        let letter = self.restore(&path, &job)?;
        let params =
            serde_json::to_value(&letter).map_err(|e| RpcError::new("internal", e.to_string()))?;
        match self.send(params, session).await {
            Ok(_) => {
                let _ = std::fs::remove_dir_all(&path);
                self.emit_changed(&[Scope::Items], None);
                Ok(())
            }
            Err(e) => {
                tracing::warn!(code = %e.code, "scheduled mail failed: {}", e.message);
                job.error = Some(if e.message.is_empty() {
                    e.code.clone()
                } else {
                    e.message.clone()
                });
                let _ = save(&path, &job);
                self.emit_changed(&[Scope::Items], None);
                Err(e)
            }
        }
    }

    /// 取消定时发送：存回邮箱的「草稿」，在那里接着改或者马上发。
    async fn unschedule(
        self: &Arc<Self>,
        session: &Arc<Session>,
        id: &str,
    ) -> Result<Value, RpcError> {
        let path = dir(self).join(id);
        let job =
            load(&path).ok_or_else(|| RpcError::new("not_found", "already sent or cancelled"))?;
        let letter = self.restore(&path, &job)?;
        let params =
            serde_json::to_value(&letter).map_err(|e| RpcError::new("internal", e.to_string()))?;
        self.save_draft(params, session).await?;
        let _ = std::fs::remove_dir_all(&path);
        self.emit_changed(&[Scope::Items], None);
        reply(json!({}))
    }

    /// 调度器每分钟调用：到时间的、没失败过的都发出去。
    pub(crate) async fn send_due_mail(self: &Arc<Self>, session: &Arc<Session>) {
        let due: Vec<String> = jobs(self)
            .into_iter()
            .filter(|(_, j)| j.error.is_none() && j.send_at <= now())
            .map(|(_, j)| j.id)
            .collect();
        for id in due {
            let _ = self.send_job(session, &id).await;
        }
    }
}
