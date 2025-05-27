use crate::{
    config::AnyQueueConfig,
    error::{Error, Result},
    job::{JobData, JobProcessor},
};
use redis::{aio::MultiplexedConnection, AsyncCommands, Client};
use std::{collections::HashMap, sync::Arc};
use tokio::{
    sync::broadcast,
    time::{self, Duration},
};

pub struct Worker {
    config: AnyQueueConfig,
    redis_client: Client,
    processors: Arc<HashMap<String, Box<dyn JobProcessor>>>,
    shutdown_tx: broadcast::Sender<()>,
}

/// Graceful shutdown handle for the worker
pub struct WorkerHandle {
    shutdown_tx: broadcast::Sender<()>,
}

impl Worker {
    pub async fn new(
        config: AnyQueueConfig,
        redis_client: Client,
        processors: Arc<HashMap<String, Box<dyn JobProcessor>>>,
    ) -> Result<Self> {
        let (shutdown_tx, _) = broadcast::channel(1);
        Ok(Self {
            config,
            redis_client,
            processors,
            shutdown_tx,
        })
    }

    /// Start the worker and return a handle for graceful shutdown
    pub async fn start_with_handle(&self) -> Result<WorkerHandle> {
        let handle = WorkerHandle {
            shutdown_tx: self.shutdown_tx.clone(),
        };
        
        Ok(handle)
    }

    pub async fn start(&self) -> Result<()> {
        self.start_internal().await
    }

    async fn start_internal(&self) -> Result<()> {
        log::info!("Worker started. Looking for jobs...");

        let mut redis_conn = self
            .redis_client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| Error::Connection(format!("Failed to get Redis connection: {}", e)))?;

        let mut consecutive_errors = 0;
        let mut shutdown_rx = self.shutdown_tx.subscribe();

        loop {
            // Check for shutdown signal
            if let Ok(_) = shutdown_rx.try_recv() {
                log::info!("Shutdown signal received. Finishing current jobs and stopping...");
                break;
            }

            let now = chrono::Utc::now().timestamp_millis();

            // Atomically claim jobs using Lua script to prevent race conditions
            let claimed_jobs: Vec<String> =
                match self.claim_jobs_atomically(&mut redis_conn, now).await {
                    Ok(jobs) => {
                        consecutive_errors = 0;
                        jobs
                    }
                    Err(e) => {
                        consecutive_errors += 1;
                        log::error!(
                            "Error claiming jobs from Redis (attempt {}): {}",
                            consecutive_errors,
                            e
                        );

                        if consecutive_errors >= self.config.max_consecutive_errors {
                            return Err(Error::Worker(format!(
                                "Too many consecutive Redis errors ({}). Worker stopping.",
                                consecutive_errors
                            )));
                        }

                        time::sleep(Duration::from_secs(5)).await;
                        Vec::new()
                    }
                };

            if !claimed_jobs.is_empty() {
                log::info!("Claimed {} jobs for processing", claimed_jobs.len());

                for job_json_str in &claimed_jobs {
                    let mut job_data: JobData = match serde_json::from_str(job_json_str) {
                        Ok(j) => j,
                        Err(e) => {
                            log::error!("Error parsing job JSON: {}. Error: {}", job_json_str, e);
                            continue;
                        }
                    };

                    // Process the job
                    let success = self.process_job(&job_data).await;

                    if !success {
                        job_data.retries += 1;
                        job_data.last_retry_at = Some(chrono::Utc::now().timestamp_millis());

                        if job_data.retries >= self.config.max_retries {
                            if let Err(e) = self
                                .add_to_dead_letter_queue(&mut redis_conn, &job_data)
                                .await
                            {
                                log::error!(
                                    "Error adding job {} to dead letter queue: {}",
                                    job_data.id,
                                    e
                                );
                            }
                        } else {
                            if let Err(e) = self.requeue_job(&mut redis_conn, &job_data).await {
                                log::error!("Error re-queueing job {}: {}", job_data.id, e);
                            }
                        }
                    }
                }
            } else {
                time::sleep(self.config.worker_poll_interval).await;
            }
        }
        
        log::info!("Worker shutdown complete.");
        Ok(())
    }

    async fn process_job(&self, job_data: &JobData) -> bool {
        log::info!(
            "Processing job: {} (type: {})",
            job_data.id,
            job_data.job_type
        );

        // Find the appropriate processor
        if let Some(processor) = self.processors.get(&job_data.job_type) {
            match processor.process(job_data).await {
                Ok(()) => {
                    log::info!("Job {} processed successfully!", job_data.id);
                    true
                }
                Err(e) => {
                    log::warn!("Job {} failed: {}", job_data.id, e);
                    false
                }
            }
        } else {
            log::error!(
                "No processor found for job type '{}' (job: {})",
                job_data.job_type,
                job_data.id
            );
            // Treat as permanent failure if no processor is available
            false
        }
    }

    async fn add_to_dead_letter_queue(
        &self,
        redis_conn: &mut MultiplexedConnection,
        job_data: &JobData,
    ) -> Result<()> {
        let job_json = serde_json::to_string(job_data)?;

        log::warn!(
            "Moving job {} to dead letter queue after {} failed attempts",
            job_data.id,
            job_data.retries
        );

        let _: () = redis_conn
            .lpush(&self.config.dead_letter_queue_key, job_json)
            .await?;
        Ok(())
    }

    async fn requeue_job(
        &self,
        redis_conn: &mut MultiplexedConnection,
        job_data: &JobData,
    ) -> Result<()> {
        let delay_duration = std::cmp::min(
            self.config.base_delay * 2_u32.pow(job_data.retries),
            self.config.max_delay,
        );

        let delay_ms = delay_duration.as_millis() as i64;
        let next_retry_timestamp_ms = chrono::Utc::now().timestamp_millis() + delay_ms;

        log::info!(
            "Job {} failed. Retrying in {:?} (retry #{}/{})",
            job_data.id,
            delay_duration,
            job_data.retries,
            self.config.max_retries
        );

        let job_json = serde_json::to_string(job_data)?;

        let _: () = redis::cmd("ZADD")
            .arg(&self.config.retry_queue_key)
            .arg(next_retry_timestamp_ms)
            .arg(job_json)
            .exec_async(redis_conn)
            .await?;

        Ok(())
    }

    /// Atomically claim jobs from the retry queue using a Lua script
    /// This prevents race conditions when multiple workers are running
    async fn claim_jobs_atomically(
        &self,
        redis_conn: &mut MultiplexedConnection,
        now: i64,
    ) -> Result<Vec<String>> {
        // Lua script that atomically finds and removes jobs from the sorted set
        // This ensures only one worker can claim each job
        let lua_script = r#"
            local retry_queue_key = KEYS[1]
            local current_time = tonumber(ARGV[1])
            local limit = tonumber(ARGV[2])
            
            -- Get jobs that are due for processing
            local jobs = redis.call('ZRANGEBYSCORE', retry_queue_key, 0, current_time, 'LIMIT', 0, limit)
            
            -- Remove the claimed jobs from the queue atomically
            if #jobs > 0 then
                for i = 1, #jobs do
                    redis.call('ZREM', retry_queue_key, jobs[i])
                end
            end
            
            return jobs
        "#;

        let claimed_jobs: Vec<String> = redis::cmd("EVAL")
            .arg(lua_script)
            .arg(1) // Number of keys
            .arg(&self.config.retry_queue_key) // KEYS[1]
            .arg(now) // ARGV[1]
            .arg(self.config.job_processing_limit) // ARGV[2]
            .query_async(redis_conn)
            .await
            .map_err(|e| Error::Worker(format!("Failed to claim jobs atomically: {}", e)))?;

        Ok(claimed_jobs)
    }
}

impl WorkerHandle {
    /// Signal the worker to shutdown gracefully
    pub async fn shutdown(&self) -> Result<()> {
        match self.shutdown_tx.send(()) {
            Ok(_) => {
                log::info!("Shutdown signal sent to worker");
                Ok(())
            }
            Err(_) => {
                log::warn!("Failed to send shutdown signal - worker may have already stopped");
                Ok(())
            }
        }
    }
}
