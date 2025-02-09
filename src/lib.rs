use std::{collections::HashMap, hash::Hash};

use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::{broadcast, mpsc};

#[derive(Error, Debug)]
pub enum SchedulerError {
    #[error("Task initialization error: {0}")]
    TaskInitialization(String),
    #[error("Task execution error: {0}")]
    TaskExecution(String),
    #[error("Task shutdown error: {0}")]
    TaskShutdown(String),
    #[error("Invalid channel: {0}")]
    InvalidChannel(String),
    #[error("Event send error: {0}")]
    EventSend(String),
    #[error("Event receive error: {0}")]
    EventReceive(String),
}

pub trait EventType: Send + Sync + Clone + std::fmt::Debug + Hash + Eq + std::fmt::Display {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionMode {
    Background,
    EventDriven,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionStatus {
    Idle,
    Running,
    Stopped,
    Failed,
}

#[async_trait]
pub trait Executable<E: EventType + 'static>: Send + Sync {
    fn name(&self) -> &str;
    fn mode(&self) -> ExecutionMode;
    fn status(&self) -> ExecutionStatus;

    fn clone_box(&self) -> Box<dyn Executable<E>>;

    async fn initialize(&mut self) -> Result<(), SchedulerError>;
    async fn execute(&mut self) -> Result<(), SchedulerError>;
    async fn shutdown(&mut self) -> Result<(), SchedulerError>;

    fn subscribed_event(&self) -> &E;
    async fn handle_event(&mut self, event: String) -> Result<(), SchedulerError>;

    fn message_sender(&self) -> Option<mpsc::Sender<String>>;
}

#[derive(Debug, Clone)]
pub struct ChannelConfig {
    pub capacity: usize,
    pub description: String,
}

pub struct EventBus<E> {
    channels: HashMap<E, broadcast::Sender<String>>,
    configs: Vec<ChannelConfig>,
}

impl<E: EventType + 'static> EventBus<E> {
    pub fn new(configs: Vec<(E, ChannelConfig)>) -> Self {
        Self {
            channels: configs
                .iter()
                .map(|c| (c.0.clone(), broadcast::channel(c.1.capacity).0))
                .collect(),
            configs: configs.iter().map(|c| c.1.clone()).collect(),
        }
    }

    pub fn add_channel(mut self, event: E, config: ChannelConfig) -> Self {
        self.channels
            .insert(event, broadcast::channel(config.capacity).0);
        self.configs.push(config);
        self
    }

    pub fn subscribe(
        &self,
        channel_event: &E,
    ) -> Result<broadcast::Receiver<String>, SchedulerError> {
        self.channels
            .get(channel_event)
            .ok_or(SchedulerError::InvalidChannel(channel_event.to_string()))
            .map(|sender| sender.subscribe())
    }

    pub fn clone_sender(
        &self,
        channel_event: &E,
    ) -> Result<broadcast::Sender<String>, SchedulerError> {
        self.channels
            .get(channel_event)
            .ok_or(SchedulerError::InvalidChannel(channel_event.to_string()))
            .cloned()
    }

    pub fn channel_count(&self) -> usize {
        self.channels.len()
    }

    pub fn channel_config(&self, channel_idx: usize) -> Option<&ChannelConfig> {
        self.configs.get(channel_idx)
    }
}

pub struct Scheduler<E> {
    tasks: Vec<Box<dyn Executable<E>>>,
    event_bus: EventBus<E>,
}

impl<E: EventType + 'static> Scheduler<E> {
    pub fn new(event_bus: EventBus<E>) -> Self {
        Self {
            tasks: Vec::new(),
            event_bus,
        }
    }

    pub fn event_bus(&self) -> &EventBus<E> {
        &self.event_bus
    }

    pub fn register_task(&mut self, task: Box<dyn Executable<E>>) {
        self.tasks.push(task);
    }

    pub async fn start(&mut self) -> Result<(), SchedulerError> {
        for task in self.tasks.iter_mut() {
            task.initialize().await?;

            match task.mode() {
                ExecutionMode::Background => {
                    let mut task_clone = task.clone_box();
                    tokio::spawn(async move {
                        loop {
                            if let Err(e) = task_clone.execute().await {
                                eprintln!("Task execution error: {}", e);
                            }
                        }
                    });
                }
                ExecutionMode::EventDriven => {
                    if let Ok(mut rx) = self.event_bus.subscribe(task.subscribed_event()) {
                        let mut task_clone = task.clone_box();

                        tokio::spawn(async move {
                            while let Ok(event) = rx.recv().await {
                                if let Err(e) = task_clone.handle_event(event).await {
                                    eprintln!("Event handling error: {}", e);
                                }
                            }
                        });
                    }
                }
            }
        }

        Ok(())
    }

    pub async fn shutdown(&mut self) -> Result<(), SchedulerError> {
        for task in self.tasks.iter_mut() {
            task.shutdown().await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    #[derive(Debug, Clone, Hash, PartialEq, Eq)]
    enum TestEvent {
        EventA,
        EventB,
    }

    impl std::fmt::Display for TestEvent {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                TestEvent::EventA => write!(f, "EventA"),
                TestEvent::EventB => write!(f, "EventB"),
            }
        }
    }

    impl EventType for TestEvent {}

    struct TestTask {
        name: String,
        mode: ExecutionMode,
        status: ExecutionStatus,
        status_test: Arc<Mutex<ExecutionStatus>>,
        event: TestEvent,
        received_events: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait]
    impl Executable<TestEvent> for TestTask {
        fn name(&self) -> &str {
            &self.name
        }

        fn mode(&self) -> ExecutionMode {
            self.mode
        }

        fn status(&self) -> ExecutionStatus {
            self.status
        }

        fn clone_box(&self) -> Box<dyn Executable<TestEvent>> {
            Box::new(Self {
                name: self.name.clone(),
                mode: self.mode,
                status: self.status,
                status_test: self.status_test.clone(),
                event: self.event.clone(),
                received_events: self.received_events.clone(),
            })
        }

        async fn initialize(&mut self) -> Result<(), SchedulerError> {
            *self.status_test.lock().await = ExecutionStatus::Running;
            self.status = ExecutionStatus::Running;
            Ok(())
        }

        async fn execute(&mut self) -> Result<(), SchedulerError> {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            Ok(())
        }

        async fn shutdown(&mut self) -> Result<(), SchedulerError> {
            *self.status_test.lock().await = ExecutionStatus::Stopped;
            self.status = ExecutionStatus::Stopped;
            Ok(())
        }

        fn subscribed_event(&self) -> &TestEvent {
            &self.event
        }

        async fn handle_event(&mut self, event: String) -> Result<(), SchedulerError> {
            self.received_events.lock().await.push(event);
            Ok(())
        }

        fn message_sender(&self) -> Option<mpsc::Sender<String>> {
            None
        }
    }

    #[tokio::test]
    async fn test_event_bus_channel_creation() {
        let event_bus = EventBus::<TestEvent>::new(vec![(
            TestEvent::EventA,
            ChannelConfig {
                capacity: 10,
                description: "Test channel".to_string(),
            },
        )]);

        assert_eq!(event_bus.channel_count(), 1);
        assert!(event_bus.channel_config(0).is_some());
    }

    #[tokio::test]
    async fn test_scheduler_background_task() {
        let status = Arc::new(Mutex::new(ExecutionStatus::Idle));

        let task = TestTask {
            name: "BackgroundTask".to_string(),
            mode: ExecutionMode::Background,
            status: ExecutionStatus::Idle,
            status_test: status.clone(),
            event: TestEvent::EventA,
            received_events: Arc::new(Mutex::new(Vec::new())),
        };

        let mut scheduler = Scheduler::new(EventBus::<TestEvent>::new(vec![]));
        scheduler.register_task(Box::new(task));
        scheduler.start().await.unwrap();

        // Give the task time to start
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        assert_eq!(*status.lock().await, ExecutionStatus::Running);
    }

    #[tokio::test]
    async fn test_scheduler_event_driven_task() {
        let event_bus = EventBus::<TestEvent>::new(vec![(
            TestEvent::EventB,
            ChannelConfig {
                capacity: 10,
                description: "Test channel".to_string(),
            },
        )]);

        let status = Arc::new(Mutex::new(ExecutionStatus::Idle));
        let received_events = Arc::new(Mutex::new(Vec::new()));
        let task = TestTask {
            name: "EventDrivenTask".to_string(),
            mode: ExecutionMode::EventDriven,
            status: ExecutionStatus::Idle,
            status_test: status.clone(),
            event: TestEvent::EventB,
            received_events: received_events.clone(),
        };

        let mut scheduler = Scheduler::new(event_bus);
        scheduler.register_task(Box::new(task));
        scheduler.start().await.unwrap();

        // Send an event
        if let Some(sender) = scheduler.event_bus.channels.get(&TestEvent::EventB) {
            sender.send("Test message".to_string()).unwrap();
        }

        // Give the task time to process
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let events = received_events.lock().await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0], "Test message");
    }
}
