use criterion::{Criterion, criterion_group, criterion_main};
use std::sync::Arc;
use std::time::Duration;
use task_orchestrator::*;
use tokio::runtime::Runtime;
use tokio::sync::Mutex;

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
enum BenchEvent {
    HighPriority,
}

impl std::fmt::Display for BenchEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BenchEvent::HighPriority => write!(f, "HighPriority"),
        }
    }
}

impl EventType for BenchEvent {}

struct BackgroundBenchmarkTask {
    name: String,
}

#[async_trait::async_trait]
impl Executable for BackgroundBenchmarkTask {
    fn name(&self) -> &str {
        &self.name
    }

    async fn initialize(&mut self) -> Result<(), SchedulerError> {
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), SchedulerError> {
        Ok(())
    }
}

#[async_trait::async_trait]
impl BackgroundTask for BackgroundBenchmarkTask {
    async fn execute(&mut self) -> Result<(), SchedulerError> {
        tokio::time::sleep(Duration::from_micros(10)).await;
        Ok(())
    }
}

struct EventDrivenBenchmarkTask {
    name: String,
    event: BenchEvent,
    received_events: Arc<Mutex<Vec<String>>>,
}

#[async_trait::async_trait]
impl Executable for EventDrivenBenchmarkTask {
    fn name(&self) -> &str {
        &self.name
    }

    async fn initialize(&mut self) -> Result<(), SchedulerError> {
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), SchedulerError> {
        Ok(())
    }
}

#[async_trait::async_trait]
impl EventTask<BenchEvent> for EventDrivenBenchmarkTask {
    fn subscribed_event(&self) -> &BenchEvent {
        &self.event
    }

    async fn handle_event(&mut self, event: String) -> Result<(), SchedulerError> {
        self.received_events.lock().await.push(event);
        Ok(())
    }
}

fn scheduler_benchmark(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("schedule_1000_background_tasks", |b| {
        b.iter(|| {
            rt.block_on(async {
                let event_bus = EventBus::<BenchEvent>::new(vec![]);
                let mut scheduler = Scheduler::new(event_bus);

                for i in 0..1000 {
                    let task = Arc::new(Mutex::new(BackgroundBenchmarkTask {
                        name: format!("bg_task_{}", i),
                    }));
                    scheduler.register_background_task(task);
                }

                scheduler.start().await.unwrap();
                tokio::time::sleep(Duration::from_millis(100)).await;
                scheduler.shutdown().await.unwrap();
            });
        });
    });

    c.bench_function("handle_10000_events", |b| {
        b.iter(|| {
            rt.block_on(async {
                let event_bus = EventBus::new(vec![(
                    BenchEvent::HighPriority,
                    ChannelConfig {
                        capacity: 10000,
                        description: "Benchmark channel".to_string(),
                    },
                )]);

                let mut scheduler = Scheduler::new(event_bus);
                let sender = scheduler
                    .event_bus()
                    .clone_sender(&BenchEvent::HighPriority)
                    .unwrap();

                let task = Arc::new(Mutex::new(EventDrivenBenchmarkTask {
                    name: "event_handler".to_string(),
                    event: BenchEvent::HighPriority,
                    received_events: Arc::new(Mutex::new(Vec::new())),
                }));
                scheduler.register_event_task(task);

                scheduler.start().await.unwrap();

                for i in 0..10000 {
                    sender.send(format!("event_{}", i)).unwrap();
                }

                tokio::time::sleep(Duration::from_millis(500)).await;
                scheduler.shutdown().await.unwrap();
            });
        });
    });
}

criterion_group!(benches, scheduler_benchmark);
criterion_main!(benches);
