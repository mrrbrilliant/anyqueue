use anyqueue::{AnyQueue, Job, Result, GenericJobProcessor};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
struct EmailJob {
    to: String,
    subject: String,
    body: String,
}

#[async_trait::async_trait]
impl Job for EmailJob {
    async fn process(&self) -> Result<()> {
        // Simulate email sending
        println!("📧 Sending email to: {}", self.to);
        println!("   Subject: {}", self.subject);
        println!("   Body: {}", self.body);
        
        // Simulate some work
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        
        // Simulate 70% success rate
        let success = rand::random::<f64>() < 0.7;
        if success {
            println!("✅ Email sent successfully!");
            Ok(())
        } else {
            println!("❌ Email failed to send");
            Err(anyqueue::Error::JobProcessing("Email service unavailable".to_string()))
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    // Create a queue with custom configuration and register processor
    let mut queue = AnyQueue::builder()
        .redis_url("redis://localhost:6379")
        .max_retries(3)
        .base_delay(std::time::Duration::from_secs(1))
        .processor(GenericJobProcessor::<EmailJob>::new())
        .build()
        .await?;

    // Clear existing jobs (for demo purposes)
    queue.clear_all_queues().await?;

    // Add some email jobs
    let jobs = vec![
        EmailJob {
            to: "alice@example.com".to_string(),
            subject: "Welcome!".to_string(),
            body: "Welcome to our service!".to_string(),
        },
        EmailJob {
            to: "bob@example.com".to_string(),
            subject: "Newsletter".to_string(),
            body: "Check out our latest updates.".to_string(),
        },
        EmailJob {
            to: "charlie@example.com".to_string(),
            subject: "Reminder".to_string(),
            body: "Don't forget about your appointment.".to_string(),
        },
    ];

    // Enqueue jobs
    for job in jobs {
        let job_id = queue.enqueue(job).await?;
        println!("📝 Enqueued job: {}", job_id);
    }

    println!("📊 Jobs in retry queue: {}", queue.retry_queue_size().await?);

    // Start processing jobs
    println!("🚀 Starting worker...");
    queue.start_worker().await?;

    Ok(())
}
